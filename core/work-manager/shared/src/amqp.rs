//! Shared AMQP utility functions and Job/Work result structures
use amqprs::{
    callbacks::{ChannelCallback, ConnectionCallback},
    channel::{BasicCancelArguments, BasicPublishArguments, Channel, QueueDeclareArguments},
    connection::{Connection, OpenConnectionArguments},
    security::SecurityCredentials,
    BasicProperties, FieldTable, FieldValue, DELIVERY_MODE_PERSISTENT,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, SystemTime};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The job result as published to the broker
#[derive(Debug, Serialize, Deserialize)]
pub struct JobResult {
    #[serde(flatten)]
    pub info: crate::object::Info,
    #[serde(serialize_with = "crate::object::serialize_meta")]
    pub relation_metadata: crate::object::Metadata,
    #[serde(flatten)]
    pub result: JobResultKind,
}

#[derive(Debug, Serialize, Deserialize)]
/// The "ok" part of the job result - present if the job succeeded
pub struct JobResultOk {
    pub symbols: Vec<String>,
    #[serde(serialize_with = "crate::object::serialize_meta")]
    pub object_metadata: crate::object::Metadata,
    pub children: Vec<JobResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
/// Indicates whether the job succeeded or failed
pub enum JobResultKind {
    ok(JobResultOk),
    error(String),
}

impl JobResult {
    pub fn get_possible_passwords(&self) -> Vec<&str> {
        let mut iters = self
            .walk()
            .filter_map(|job_result| {
                if let JobResultKind::ok(res_ok) = &job_result.result {
                    Some(res_ok)
                } else {
                    None
                }
            })
            .filter_map(|res_ok| {
                if let Some(serde_json::Value::Array(passwords)) =
                    res_ok.object_metadata.get("possible_passwords")
                {
                    Some(passwords.iter().filter_map(|p| {
                        if let serde_json::Value::String(p) = p {
                            Some(p)
                        } else {
                            None
                        }
                    }))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut res: Vec<&str> = Vec::new();
        loop {
            let mut complete = true;
            for pwd_iter in iters.iter_mut() {
                if let Some(pwd) = pwd_iter.next() {
                    complete = false;
                    if seen.insert(pwd) {
                        res.push(pwd);
                    }
                }
            }
            if complete {
                break;
            }
        }
        res
    }

    pub fn get_children<'a>(&'a self) -> std::slice::Iter<'a, JobResult> {
        if let JobResultKind::ok(res) = &self.result {
            res.children.as_slice().iter()
        } else {
            [].iter()
        }
    }

    pub fn walk<'a>(&'a self) -> ResultIterator<'a> {
        let res = ResultIterator {
            current_iter: ResultIteratorCurrent::Parent(self),
            parent_iter: None,
        };
        res
    }

    pub fn has_symbol(&self, symbol: &str) -> bool {
        if let JobResultKind::ok(res) = &self.result {
            res.symbols.iter().any(|s| s == symbol)
        } else {
            false
        }
    }
}

enum ResultIteratorCurrent<'a> {
    Parent(&'a JobResult),
    Children(std::slice::Iter<'a, JobResult>),
}

pub struct ResultIterator<'a> {
    current_iter: ResultIteratorCurrent<'a>,
    parent_iter: Option<Box<ResultIterator<'a>>>,
}

impl<'a> Iterator for ResultIterator<'a> {
    type Item = &'a JobResult;

    fn next(&mut self) -> Option<Self::Item> {
        match self.current_iter {
            ResultIteratorCurrent::Parent(parent) => {
                self.current_iter = ResultIteratorCurrent::Children(parent.get_children());
                Some(parent)
            }
            ResultIteratorCurrent::Children(ref mut child_it) => match child_it.next() {
                Some(job) => {
                    let old_self = std::mem::replace(
                        self,
                        Self {
                            current_iter: ResultIteratorCurrent::Parent(job),
                            parent_iter: None,
                        },
                    );
                    self.parent_iter = Some(Box::new(old_self));
                    self.next()
                }
                None => match self.parent_iter {
                    Some(ref mut parent) => parent.next(),
                    None => None,
                },
            },
        }
    }
}

struct ConnectionCB;
#[async_trait::async_trait]
impl ConnectionCallback for ConnectionCB {
    async fn close(
        &mut self,
        _connection: &Connection,
        close: amqprs::Close,
    ) -> Result<(), amqprs::error::Error> {
        warn!("Broker closed the connection: {}", close);
        Ok(())
    }
    async fn blocked(&mut self, _connection: &Connection, reason: String) {
        debug!("Broker connection blocked: {}", reason);
    }
    async fn unblocked(&mut self, _connection: &Connection) {
        debug!("Broker connection unblocked");
    }
}

struct ChannelCB;
#[async_trait::async_trait]
impl ChannelCallback for ChannelCB {
    async fn close(
        &mut self,
        _channel: &Channel,
        close: amqprs::CloseChannel,
    ) -> Result<(), amqprs::error::Error> {
        warn!("Channel closed: {}", close);
        Ok(())
    }
    async fn cancel(
        &mut self,
        _channel: &Channel,
        _cancel: amqprs::Cancel,
    ) -> Result<(), amqprs::error::Error> {
        Ok(())
    }
    async fn flow(
        &mut self,
        _channel: &Channel,
        _active: bool,
    ) -> Result<bool, amqprs::error::Error> {
        Ok(true)
    }
    async fn publish_ack(&mut self, _channel: &Channel, _ack: amqprs::Ack) {}
    async fn publish_nack(&mut self, _channel: &Channel, _nack: amqprs::Nack) {}
    async fn publish_return(
        &mut self,
        _channel: &Channel,
        _ret: amqprs::Return,
        _basic_properties: BasicProperties,
        _content: Vec<u8>,
    ) {
    }
}

/// Connects to the message broker
pub async fn connect(
    broker_cfg: &crate::config::BrokerConfig,
) -> Result<Connection, Box<dyn std::error::Error>> {
    let mut args = OpenConnectionArguments::default();
    let host = broker_cfg.host.as_deref();
    if let Some(host) = host {
        args.host(host);
    }
    let port = broker_cfg.port;
    if let Some(port) = port {
        args.port(port);
    }
    if let Some(user) = broker_cfg.user.as_deref() {
        if let Some(pass) = broker_cfg.pass.as_deref() {
            args.credentials(SecurityCredentials::new_plain(user, pass));
        } else {
            let msg = "Invalid AMQP configuration (password not provided)";
            error!("{}", msg);
            return Err(msg.into());
        }
    } else if broker_cfg.pass.is_some() {
        let msg = "Invalid AMQP configuration (password set without username)";
        error!("{}", msg);
        return Err(msg.into());
    }
    args.heartbeat(10);
    let host = host.unwrap_or("localhost");
    let port = port.unwrap_or(5672);
    debug!("Connecting to message broker {}:{}...", host, port);
    let conn = Connection::open(&args).await.map_err(|e| {
        error!(
            "Failed to connect to message broker {}:{} - {}",
            host, port, e
        );
        e
    })?;
    if let Err(e) = conn.register_callback(ConnectionCB).await {
        close_connection(conn).await;
        Err(e.into())
    } else {
        info!("Successfully connected to message broker {}:{}", host, port);
        debug!(
            "Connection {}: {} channels, frame size {}, heartbeat {}",
            conn.connection_name(),
            conn.channel_max(),
            conn.frame_max(),
            conn.heartbeat()
        );
        debug!("{:#?}", conn.server_properties());
        Ok(conn)
    }
}

/// Cleanly disconnects from the broker
pub async fn close_connection(conn: Connection) {
    if let Err(e) = conn.close().await {
        warn!("Failed to close connection: {}", e)
    }
}

/// Channel open utility fn
pub async fn open_channel(conn: &Connection) -> Result<Channel, Box<dyn std::error::Error>> {
    debug!("Opening channel...");
    let chan = conn.open_channel(None).await.map_err(|e| {
        error!("Failed to open channel: {}", e);
        e
    })?;
    if let Err(e) = chan.register_callback(ChannelCB).await {
        error!("Failed to register channel callbacks: {}", e);
        close_channel(chan).await;
        Err(Box::new(e))
    } else {
        debug!("Channel opened successfully (id: {})", chan.channel_id());
        Ok(chan)
    }
}

/// A forgiving unsubscribe (used on cleanup paths)
pub async fn unsubscribe(chan: &Channel, ctag: &str) {
    if let Err(e) = chan.basic_cancel(BasicCancelArguments::new(ctag)).await {
        warn!("Failed to cancel subscription with ctag {}: {}", ctag, e);
    }
}

/// A forgiving channel closer (used on cleanup paths)
pub async fn close_channel(chan: Channel) {
    if let Err(e) = chan.close().await {
        warn!("Failed to close channel: {}", e)
    }
}

/// Posts a job request
pub async fn publish_job_request(
    channel: &Channel,
    object_ref: crate::object::DescriptorRef<'_>,
    ttl: Duration,
    correlation_id: Option<String>,
    reply_queue: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let correlation_id =
        correlation_id.unwrap_or_else(|| crate::utils::random_string(crate::MSG_CORRID_LEN));
    let expiration_ts: u64 = (SystemTime::now() + ttl)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut headers = FieldTable::new();
    headers.insert(
        "expiration_ts".try_into().unwrap(),
        FieldValue::T(expiration_ts),
    );
    let bprops = BasicProperties::default()
        .with_content_type(crate::MSG_CONTENT_TYPE)
        .with_correlation_id(&correlation_id)
        .with_headers(headers)
        .with_message_type(crate::REQUEST_TYPE)
        .with_delivery_mode(DELIVERY_MODE_PERSISTENT)
        .with_reply_to(reply_queue.unwrap_or(crate::RESULTS_QUEUE_NAME))
        .finish();
    let args = BasicPublishArguments::default()
        .routing_key(object_ref.info.request_queue())
        .finish();
    let request_json = serde_json::to_string(&object_ref).unwrap();
    channel
        .basic_publish(bprops, request_json.into_bytes(), args)
        .await
        .map_err(|e| {
            error!(
                "Failed to publish job request for object \"{}\" to {}: {}",
                object_ref.info.object_id,
                object_ref.info.request_queue(),
                e
            );
            e
        })?;
    debug!(
        "Posted job request for object \"{}\" to {} with correlation_id {}",
        object_ref.info.object_id,
        object_ref.info.request_queue(),
        correlation_id
    );
    Ok(correlation_id)
}

pub async fn declare_director_queue(channel: &Channel) -> Result<(), Box<dyn std::error::Error>> {
    debug!(
        "Declaring the director result queue \"{}\"...",
        crate::DIRECTOR_QUEUE_NAME
    );
    let mut args = FieldTable::new();
    args.insert("x-queue-type".try_into().unwrap(), "quorum".into());
    let qargs = QueueDeclareArguments::new(crate::DIRECTOR_QUEUE_NAME)
        .durable(true)
        .arguments(args)
        .finish();

    let (_, message_count, consumer_count) = channel
        .queue_declare(qargs)
        .await
        .map_err(|e| {
            error!(
                "Failed to declare the director queue \"{}\": {}",
                crate::DIRECTOR_QUEUE_NAME,
                e
            );
            e
        })?
        .unwrap();
    debug!(
        "Director queue \"{}\" successfully declared ({} messages, {} consumers)",
        crate::DIRECTOR_QUEUE_NAME,
        message_count,
        consumer_count
    );
    Ok(())
}

pub async fn request_apply_scenarios(
    work_id: String,
    channel: &Channel,
) -> Result<(), amqprs::error::Error> {
    let bprops = BasicProperties::default()
        .with_content_type(super::MSG_CONTENT_TYPE)
        .with_message_type(super::SC_PROCESS_TYPE)
        .finish();
    let args = BasicPublishArguments::default()
        .routing_key(super::DIRECTOR_QUEUE_NAME.to_string())
        .finish();
    let request_json = serde_json::to_string(&super::scene::DirectorRequest {
        work_id: work_id.to_string(),
    })
    .unwrap();
    channel
        .basic_publish(bprops, request_json.into_bytes(), args)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_walk() {
        let obj_json = r#"
            {
                "org": "test",
                "object_id": "a",
                "object_type": "Email",
                "object_subtype": null,
                "recursion_level": 1,
                "size": 1,
                "hashes": {},
                "ctime": 1.0,
                "relation_metadata": {},
                "ok": {
                    "symbols": [],
                    "object_metadata": {
                        "possible_passwords": ["1", "2", "6"]
                    },
                    "children": [
                        {
                            "org": "test",
                            "object_id": "a/a",
                            "object_type": "Email",
                            "object_subtype": null,
                            "recursion_level": 1,
                            "size": 1,
                            "hashes": {},
                            "ctime": 1.0,
                            "relation_metadata": {},
                            "ok": {
                                "symbols": [],
                                "object_metadata": {
                                    "possible_passwords": ["2", "4"]
                                },
                                "children": [
                                    {
                                        "org": "test",
                                        "object_id": "a/a/a",
                                        "object_type": "Email",
                                        "object_subtype": null,
                                        "recursion_level": 1,
                                        "size": 1,
                                        "hashes": {},
                                        "ctime": 1.0,
                                        "relation_metadata": {},
                                        "ok": {
                                            "symbols": [ "ENCRYPTED" ],
                                            "object_metadata": {
                                                "possible_passwords": ["3"]
                                            },
                                            "children": [
                                           ]
                                        }
                                    },
                                    {
                                        "org": "test",
                                        "object_id": "a/a/b",
                                        "object_type": "Email",
                                        "object_subtype": null,
                                        "recursion_level": 1,
                                        "size": 1,
                                        "hashes": {},
                                        "ctime": 1.0,
                                        "relation_metadata": {},
                                        "ok": {
                                            "symbols": [],
                                            "object_metadata": {
                                                "possible_passwords": ["3", "2", "4"]
                                            },
                                            "children": [
                                                {
                                                    "org": "test",
                                                    "object_id": "a/a/b/a",
                                                    "object_type": "Email",
                                                    "object_subtype": null,
                                                    "recursion_level": 1,
                                                    "size": 1,
                                                    "hashes": {},
                                                    "ctime": 1.0,
                                                    "relation_metadata": {},
                                                    "ok": {
                                                        "symbols": [],
                                                        "object_metadata": {
                                                            "possible_passwords": ["2", "5"]
                                                        },
                                                        "children": [
                                                       ]
                                                    }
                                                }
                                            ]
                                        }
                                    }
                               ]
                            }
                        },
                        {
                            "org": "test",
                            "object_id": "a/b",
                            "object_type": "Email",
                            "object_subtype": null,
                            "recursion_level": 1,
                            "size": 1,
                            "hashes": {},
                            "ctime": 1.0,
                            "relation_metadata": {},
                            "error": "some failure here"
                        },
                        {
                            "org": "test",
                            "object_id": "a/b",
                            "object_type": "Email",
                            "object_subtype": null,
                            "recursion_level": 1,
                            "size": 1,
                            "hashes": {},
                            "ctime": 1.0,
                            "relation_metadata": {},
                            "ok": {
                                "symbols": [ "ENCRYPTED", "DECRYPTED" ],
                                "object_metadata": {
                                    "possible_passwords": ["3", "6", "7"]
                                },
                                "children": []
                            }
                        }
                    ]
                }
            }
        "#;
        let obj: JobResult = serde_json::from_str(obj_json).unwrap();
        let mut it = obj.walk();
        assert_eq!(it.next().unwrap().info.object_id, "a");
        assert_eq!(it.next().unwrap().info.object_id, "a/a");
        assert_eq!(it.next().unwrap().info.object_id, "a/a/a");
        assert_eq!(it.next().unwrap().info.object_id, "a/a/b");
        assert_eq!(it.next().unwrap().info.object_id, "a/a/b/a");
        assert!(matches!(it.next().unwrap().result, JobResultKind::error(_)));
        assert_eq!(it.next().unwrap().info.object_id, "a/b");
        assert!(it.next().is_none());
        assert_eq!(
            obj.get_possible_passwords(),
            ["1", "2", "3", "4", "5", "6", "7"]
        );
        assert!(obj
            .walk()
            .any(|res| res.has_symbol("ENCRYPTED") && !res.has_symbol("DECRYPTED")));
        assert!(obj
            .walk()
            .any(|res| res.has_symbol("ENCRYPTED") && res.has_symbol("DECRYPTED")));
    }
}
