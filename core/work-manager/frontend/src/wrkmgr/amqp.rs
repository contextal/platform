//! Message broker AMQP communication
//!
//! This module handles the communication with the message broker in terms of:
//! - Establishing and cleanly closing the connection, channels, subscriptions, etc
//! - Receiving job requests
//! - Posting child job requests
//! - Receiving child job results
//! - Publishing job results
//!
//! It additionally contains a few support structures and utility functions
use super::metrics;
use crate::config::Config;
use amqprs::{
    channel::{
        BasicAckArguments, BasicConsumeArguments, BasicPublishArguments, BasicRejectArguments,
        Channel, ConsumerMessage, QueueDeclareArguments,
    },
    connection::Connection,
    BasicProperties, FieldTable, FieldValue,
};
use shared::{
    amqp::{self, JobResult, JobResultKind, JobResultOk},
    object, utils,
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc::UnboundedReceiver;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

pub trait TimeRemaining {
    fn time_remaining(&self) -> Result<Duration, Duration>;
}

impl TimeRemaining for SystemTime {
    fn time_remaining(&self) -> Result<Duration, Duration> {
        // Note: elapsed() is used in reverse, so Ok() and Err() are also in reverse
        match self.elapsed() {
            Err(e) => Ok(e.duration()),
            Ok(sec) => Err(sec),
        }
    }
}

/// A support struct holding the request object and related AMQP info
#[derive(Debug)]
pub struct JobRequest {
    /// The AMQP delivery tag
    delivery_tag: u64,
    /// The queue to send results to
    reply_to: String,
    /// The id to match replies to requests
    correlation_id: String,
    /// The timestamp at which the full work expires
    expiration_ts: SystemTime,
    /// The number of delivery attempts
    pub delivery_count: i64,
    /// The reception time
    recvd_at: std::time::Instant,
    /// The object descriptor received in the request
    pub object: object::Descriptor,
}

impl TimeRemaining for JobRequest {
    fn time_remaining(&self) -> Result<Duration, Duration> {
        match self.expiration_ts.time_remaining() {
            // Lingering (allow for small time jumps)
            Err(sec) if sec.as_millis() < 2000 => Ok(Duration::from_secs(2)),
            // Either not expired or expired beyond lingering
            v => v,
        }
    }
}

/// A support struct holding the response object and its (to-be-processed) children
///
/// As child job get processed the children are morphed from [`PendingChild`]
/// into [`JobResult`]
#[derive(Debug)]
pub struct PendingResult {
    delivery_tag: u64,
    reply_to: String,
    correlation_id: String,
    expiration_ts: SystemTime,
    result: JobResult,
    children: Vec<PendingChildKind>,
}

/// A child for which a result is pending
#[derive(Debug)]
pub struct PendingChild {
    info: object::Info,
    correlation_id: String,
    symbols: Vec<String>,
    relation_metadata: object::Metadata,
    max_recursion: u32,
}

impl PendingChild {
    fn new(
        info: object::Info,
        symbols: Vec<String>,
        relation_metadata: object::Metadata,
        max_recursion: u32,
    ) -> Self {
        Self {
            info,
            correlation_id: utils::random_string(shared::MSG_CORRID_LEN),
            symbols,
            relation_metadata,
            max_recursion,
        }
    }
}

impl<'a> From<&'a PendingChild> for object::DescriptorRef<'a> {
    fn from(pchild: &'a PendingChild) -> Self {
        Self {
            info: &pchild.info,
            symbols: &pchild.symbols,
            relation_metadata: &pchild.relation_metadata,
            max_recursion: pchild.max_recursion,
        }
    }
}

/// Indicates whether the child job is complete or pending
#[derive(Debug)]
pub enum PendingChildKind {
    Complete(JobResult),
    Pending(PendingChild),
}

impl PendingChildKind {
    pub fn new(
        info: object::Info,
        symbols: Vec<String>,
        relation_metadata: object::Metadata,
        max_recursion: u32,
    ) -> Self {
        if info.is_skipped() || info.is_empty() {
            Self::Complete(JobResult {
                info,
                relation_metadata,
                result: JobResultKind::ok(JobResultOk {
                    symbols,
                    object_metadata: object::Metadata::new(),
                    children: Vec::new(),
                }),
            })
        } else {
            Self::Pending(PendingChild::new(
                info,
                symbols,
                relation_metadata,
                max_recursion,
            ))
        }
    }
}

impl PendingResult {
    /// Creates a new successful PendingResult
    pub fn new_ok(
        job_request: JobRequest,
        symbols: Vec<String>,
        object_metadata: object::Metadata,
        children: Vec<PendingChildKind>,
    ) -> Self {
        let processing_time = job_request.recvd_at.elapsed();
        metrics::set_job_processing_time(&processing_time);
        Self {
            delivery_tag: job_request.delivery_tag,
            reply_to: job_request.reply_to,
            correlation_id: job_request.correlation_id,
            expiration_ts: job_request.expiration_ts,
            result: JobResult {
                info: job_request.object.info,
                // Note: carried over unmodified
                relation_metadata: job_request.object.relation_metadata,
                result: JobResultKind::ok(JobResultOk {
                    symbols,
                    // Note: merged with backend results
                    object_metadata,
                    children: Vec::with_capacity(children.len()),
                }),
            },
            children,
        }
    }

    /// Creates a new failed PendingResult
    pub fn new_err(job_request: JobRequest, error: String) -> Self {
        Self {
            delivery_tag: job_request.delivery_tag,
            reply_to: job_request.reply_to,
            correlation_id: job_request.correlation_id,
            expiration_ts: job_request.expiration_ts,
            result: JobResult {
                info: job_request.object.info,
                // Note: carried over unmodified
                relation_metadata: job_request.object.relation_metadata,
                result: JobResultKind::error(error),
            },
            children: Vec::new(),
        }
    }

    /// Checks if all the child jobs are complete
    fn is_complete(&self) -> bool {
        self.children
            .iter()
            .all(|ref c| matches!(c, PendingChildKind::Complete(_)))
    }

    /// Checks if the job is expired
    fn is_expired(&self) -> bool {
        self.time_remaining().is_err()
    }
}

impl TimeRemaining for PendingResult {
    fn time_remaining(&self) -> Result<Duration, Duration> {
        self.expiration_ts.time_remaining()
    }
}

/// An AMQP interface to the message broker
///
/// For practical reasons this is internally split between a "current object" interface
/// (see [`ObjQueue`]) and a child job interface (see [`ChildQueue`])
///
/// Note: all the error paths are pedantic and will insist on explicit close
/// rather than relying on Drop. This behavior is recommended by the docs.
pub struct Broker {
    connection: Connection,
    objq: ObjQueue,
    childq: ChildQueue,
}

impl Broker {
    /// Creates a new interface to the message broker
    pub async fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Connect to broker
        let connection = amqp::connect(&config.broker).await?;

        // Create the child channel and queue
        let childq = match ChildQueue::new(&connection, config.get_worker_type()).await {
            Err(e) => {
                amqp::close_connection(connection).await;
                return Err(e);
            }
            Ok(v) => v,
        };

        // Create the object channel and queue
        let request_queue = utils::get_queue_for(config.get_worker_type());
        let objq = match ObjQueue::new(&connection, &request_queue).await {
            Err(e) => {
                childq.close().await;
                amqp::close_connection(connection).await;
                return Err(e);
            }
            Ok(v) => v,
        };

        Ok(Self {
            connection,
            objq,
            childq,
        })
    }

    /// Cleanly reject all tags, unsubscribes, closes channels, and disconnects
    pub async fn close(mut self) {
        for job in self.childq.jobs.values() {
            self.objq.reject_tag(job.delivery_tag).await.ok();
        }
        self.objq.close().await;
        self.childq.close().await;
        amqp::close_connection(self.connection).await;
    }

    /// Rejects a previously received job request message
    ///
    /// The message will be re-published later
    pub async fn reject_job_request(
        &mut self,
        job_request: &JobRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        metrics::job_rescheduled();
        self.objq.reject_tag(job_request.delivery_tag).await
    }

    /// Retrieves the next available job request and additionally monitors child job results
    ///
    /// Note: a [`None`] result indicates a fatal condition
    pub async fn get_request(&mut self) -> Option<JobRequest> {
        loop {
            tokio::select!(
                request = self.objq.get_request() => {
                    return request
                },
                job = self.childq.get_complete_job() => {
                    let job = job?;
                    if self.objq.publish_job_result(job).await.is_err() {
                        return None;
                    }
                }
            )
        }
    }

    /// Asynchronously handles child job result receptions and job result publication
    ///
    /// Never returns except on a fatal condition
    ///
    /// Note: this is intended to be called from the main loop whenever a blocking async
    /// operation is in progress (e.g. waiting for the backend) inside a
    /// `tokio::select!()` block
    pub async fn process_child_queue(&mut self) {
        loop {
            if let Some(job) = self.childq.get_complete_job().await {
                if self.objq.publish_job_result(job).await.is_ok() {
                    continue;
                }
            }
            return;
        }
    }

    /// Sets the provided result up for publishing
    ///
    /// Publishing will happen asynchronously when all its child jobs are completed
    ///
    /// Note: an [`Error`](std::error::Error) result indicates a fatal condition
    pub async fn publish_result_when_complete(
        &mut self,
        pending: PendingResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if pending.is_complete() || pending.is_expired() {
            info!(
                "Job for object \"{}\" is complete",
                pending.result.info.object_id
            );
            self.objq.publish_job_result(pending).await
        } else {
            info!(
                "Job for object \"{}\" queued while awaiting for {} children results",
                pending.result.info.object_id,
                pending.children.len()
            );
            self.childq.add_pending_job(pending).await
        }
    }
}

/// Manages messaging operations related to the currently processed object
///
/// Tasks:
/// - Receive job requests from parent to this worker (manual ack) - see
///   [`get_request()`](Self::get_request)
/// - Publish our job results - see [`publish_job_result()`](Self::publish_job_result)
struct ObjQueue {
    channel: Channel,
    ctag: String,
    subscription: UnboundedReceiver<ConsumerMessage>,
}

impl ObjQueue {
    /// Creates a new channel declares the proper (durable) queue and subscribes to it
    async fn new(conn: &Connection, queue_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Create channel
        let channel = amqp::open_channel(conn).await?;

        match async {
            // Create or join the work request queue
            debug!("Declaring the job request queue \"{}\"...", queue_name);
            let mut args = FieldTable::new();
            args.insert("x-queue-type".try_into().unwrap(), "quorum".into());
            args.insert(
                "x-message-ttl".try_into().unwrap(),
                FieldValue::i((shared::MAX_WORK_TTL * 2).as_millis().try_into().unwrap()),
            );
            let qargs = QueueDeclareArguments::new(queue_name)
                .durable(true)
                .arguments(args)
                .finish();
            let (_, message_count, consumer_count) = channel
                .queue_declare(qargs)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to declare the job request queue \"{}\": {}",
                        queue_name, e
                    );
                    e
                })?
                .unwrap();
            debug!(
                "Job request queue \"{}\" successfully declared ({} messages, {} consumers)",
                queue_name, message_count, consumer_count
            );

            // Subscribe to the job request queue
            let args = BasicConsumeArguments::default()
                .queue(queue_name.to_string())
                .auto_ack(false)
                .finish();
            let (ctag, subscription) = channel.basic_consume_rx(args).await.map_err(|e| {
                error!(
                    "Failed to subscribe to the job request queue \"{}\": {}",
                    queue_name, e
                );
                e
            })?;
            debug!(
                "Subscribed to the job request queue \"{}\" with ctag {}",
                queue_name, ctag
            );
            Ok((ctag, subscription))
        }
        .await
        {
            Ok((ctag, subscription)) => Ok(Self {
                channel,
                ctag,
                subscription,
            }),
            Err(e) => {
                amqp::close_channel(channel).await;
                Err(e)
            }
        }
    }

    /// Cleanly unsubscribes and closes the channel
    async fn close(self) {
        amqp::unsubscribe(&self.channel, &self.ctag).await;
        amqp::close_channel(self.channel).await;
    }

    /// Retrieves the first available job request from the queue
    ///
    /// It additionally validates the message, its properties and body; non conformant
    /// messages are ignored and rejected (i.e. left in the queue)
    ///
    /// Note: a [`None`] result indicates a fatal condition
    async fn get_request(&mut self) -> Option<JobRequest> {
        loop {
            debug!("Awaiting job requests...");
            // Receive broker message
            let msg = match self.subscription.recv().await {
                Some(m) => m,
                None => {
                    error!("Failed to receive job request: channel closed by server");
                    return None;
                }
            };
            let recvd_at = std::time::Instant::now();
            let deliver = msg.deliver.as_ref().unwrap();
            let delivery_tag = deliver.delivery_tag();
            let bprops = msg.basic_properties.as_ref().unwrap();

            debug!(
                "Received job request message with delivery_tag={}\n",
                delivery_tag
            );

            let delivery_count = if let Some(FieldValue::l(dc)) = bprops
                .headers()
                .and_then(|hdrs| hdrs.get(&"x-delivery-count".try_into().unwrap()))
            {
                *dc
            } else {
                0
            };
            if delivery_count > 1000 {
                // Drop blatanlty stuck message
                warn!("Inflooping message dropped");
                if self.ack_tag(delivery_tag).await.is_err() {
                    return None;
                }
                continue;
            }

            // Sanitize message
            // If not well formed the message will be rejected
            // and will eventually expire
            match bprops.message_type() {
                Some(v) if v == shared::REQUEST_TYPE => {}
                _ => {
                    warn!("Job request message has missing or invalid type");
                    if self.reject_tag(delivery_tag).await.is_err() {
                        return None;
                    }
                    continue;
                }
            };
            match bprops.content_type() {
                Some(v) if v == shared::MSG_CONTENT_TYPE => {}
                _ => {
                    warn!("Job request message has missing or invalid content_type");
                    if self.reject_tag(delivery_tag).await.is_err() {
                        return None;
                    }
                    continue;
                }
            }
            let reply_to = match bprops.reply_to() {
                Some(v) => v,
                None => {
                    warn!("Job request message has missing reply_to");
                    if self.reject_tag(delivery_tag).await.is_err() {
                        return None;
                    }
                    continue;
                }
            };
            let correlation_id = match bprops.correlation_id() {
                Some(v) => v,
                None => {
                    warn!("Job request message has no correlation_id");
                    if self.reject_tag(delivery_tag).await.is_err() {
                        return None;
                    }
                    continue;
                }
            };
            let expiration_ts = if let Some(FieldValue::T(ts)) = bprops
                .headers()
                .and_then(|hdrs| hdrs.get(&"expiration_ts".try_into().unwrap()))
            {
                SystemTime::UNIX_EPOCH + Duration::from_secs(*ts)
            } else {
                warn!("Job request message has missing or invalid expiration_ts header");
                if self.reject_tag(delivery_tag).await.is_err() {
                    return None;
                }
                continue;
            };
            let object: object::Descriptor =
                match serde_json::from_slice(msg.content.as_ref().unwrap()) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("Job request message has invalid payload: {}", e);
                        if self.reject_tag(delivery_tag).await.is_err() {
                            return None;
                        }
                        continue;
                    }
                };

            let ret = JobRequest {
                delivery_tag,
                reply_to: reply_to.to_string(),
                correlation_id: correlation_id.to_string(),
                expiration_ts,
                delivery_count,
                recvd_at,
                object,
            };
            debug!("Job request received: {:#?}", ret);
            return Some(ret);
        }
    }

    /// Rejects a job request message which will be re-published later
    ///
    /// Note: an [`Error`](std::error::Error) result indicates a fatal condition
    async fn reject_tag(&mut self, delivery_tag: u64) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending reject for delivery_tag={}", delivery_tag);
        self.channel
            .basic_reject(BasicRejectArguments::new(delivery_tag, true))
            .await
            .map_err(|e| {
                error!(
                    "Failed to reject job request with delivery tag {}: {}",
                    delivery_tag, e
                );
                e
            })?;
        Ok(())
    }

    /// Acknowledges a job request message, removing it from the broker
    ///
    /// Note: a [`Error`](std::error::Error) result indicates a fatal condition
    async fn ack_tag(&mut self, delivery_tag: u64) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending ack for delivery_tag={}", delivery_tag);
        self.channel
            .basic_ack(BasicAckArguments::new(delivery_tag, false))
            .await
            .map_err(|e| {
                error!(
                    "Failed to ack job request with delivery tag {}: {}",
                    delivery_tag, e
                );
                e
            })?;
        Ok(())
    }

    /// Posts a complete job result to the result message queue
    pub async fn publish_job_result(
        &mut self,
        mut pending: PendingResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let JobResultKind::ok(resok) = &mut pending.result.result {
            resok.children = pending
                .children
                .into_iter()
                .map(|c| {
                    match c {
                        PendingChildKind::Complete(jobres) => jobres,
                        PendingChildKind::Pending(pres) => {
                            // Expired jobs land in here
                            JobResult {
                                info: pres.info,
                                // Note: carried over unmodified
                                relation_metadata: pres.relation_metadata,
                                result: JobResultKind::error("Time out".to_owned()),
                            }
                        }
                    }
                })
                .collect();
        }
        if pending.correlation_id.len() == shared::MSG_CORRID_LEN {
            let elapsed = pending
                .result
                .info
                .work_creation_time()
                .elapsed()
                .unwrap_or_else(|_| std::time::Duration::from_secs(0));
            metrics::set_work_processing_time(elapsed);
        }

        let bprops = BasicProperties::default()
            .with_content_type(shared::MSG_CONTENT_TYPE)
            .with_correlation_id(&pending.correlation_id)
            .with_message_type(shared::RESULT_TYPE)
            .finish();
        // NOTE: neither the mandatory nor the immediate args are of any use:
        // the former won't fail the publish but simply bounce it back
        // the latter is not implemented in rabbit
        debug!(
            "Posting job result to {}: {:#?}",
            pending.reply_to, pending.result
        );
        let args = BasicPublishArguments::default()
            .routing_key(pending.reply_to)
            .finish();
        let request_json = serde_json::to_string(&pending.result).unwrap();
        let ret = self
            .channel
            .basic_publish(bprops, request_json.into_bytes(), args)
            .await
            .map_err(|e| e.into());
        if ret.is_ok() {
            info!(
                "Published job result for object \"{}\"",
                pending.result.info.object_id
            );
            self.ack_tag(pending.delivery_tag).await?;
        } else {
            error!(
                "Failed to publish job result for object \"{}\": {}",
                pending.result.info.object_id,
                ret.as_ref().unwrap_err()
            );
            self.reject_tag(pending.delivery_tag).await?;
        }
        metrics::job_completed();
        ret
    }
}

/// Manages messaging operations related to the children of the current object
///
/// Tasks:
/// - Post child job requests
/// - Receive child job results (exclusive queue)
struct ChildQueue {
    channel: Channel,
    queue: String,
    ctag: String,
    subscription: UnboundedReceiver<ConsumerMessage>,
    jobs: HashMap<String, PendingResult>,
}

impl ChildQueue {
    /// Creates a new (exclusive) channel, declares the proper queue and subscribes to it
    async fn new(conn: &Connection, worker_type: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Create result channel
        let channel = amqp::open_channel(conn).await?;

        match async {
            // Create exclusive queue (name assigned by broker)
            debug!("Declaring the job result queue...");
            let mut tbl = FieldTable::new();
            tbl.insert("worker_type".try_into().unwrap(), worker_type.into());
            let qargs = QueueDeclareArguments::default()
                .arguments(tbl)
                .exclusive(true)
                .auto_delete(true)
                .finish();
            let (queue, message_count, consumer_count) = channel
                .queue_declare(qargs)
                .await
                .map_err(|e| {
                    error!("Failed to declare the job result queue: {}", e);
                    e
                })?
                .unwrap();
            debug!(
                "Job result queue \"{}\" declared ({} messages, {} consumers)",
                queue, message_count, consumer_count
            );

            // Subscribe to the result queue
            let args = BasicConsumeArguments::default()
                .queue(queue.clone())
                .auto_ack(true)
                .finish();
            let (ctag, subscription) = channel.basic_consume_rx(args).await.map_err(|e| {
                error!(
                    "Failed to subscribe to the job result queue \"{}\": {}",
                    queue, e
                );
                e
            })?;
            debug!(
                "Subscribed to the job result queue \"{}\" with ctag {}",
                queue, ctag
            );
            Ok((queue, ctag, subscription))
        }
        .await
        {
            Ok((queue, ctag, subscription)) => Ok(Self {
                channel,
                queue,
                ctag,
                subscription,
                jobs: HashMap::new(),
            }),
            Err(e) => {
                amqp::close_channel(channel).await;
                Err(e)
            }
        }
    }

    /// Cleanly unsubscribes and closes the channel
    ///
    /// The queue will automatically be dropped by the message broker
    async fn close(self) {
        amqp::unsubscribe(&self.channel, &self.ctag).await;
        amqp::close_channel(self.channel).await;
    }

    /// Dispatches any child job request and stores the pending result internally
    ///
    /// Note: an [`Error`](std::error::Error) result indicates a fatal condition
    async fn add_pending_job(
        &mut self,
        pending: PendingResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = utils::random_string(shared::MSG_CORRID_LEN);
        for child in pending.children.iter() {
            if let PendingChildKind::Pending(pchild) = child {
                self.publish_job_request(&key, pchild, &pending.expiration_ts)
                    .await?;
                info!(
                    "Posted job request for object \"{}\" child of \"{}\"",
                    pchild.info.object_id, pending.result.info.object_id
                );
            }
        }
        debug!("New pending job added: {:#?}", pending);
        self.jobs.insert(key, pending);
        metrics::set_waiting_count(self.jobs.len());
        Ok(())
    }

    /// Posts a child job request
    ///
    /// Note: an [`Error`](std::error::Error) result indicates a fatal condition
    async fn publish_job_request(
        &self,
        key: &str,
        child: &PendingChild,
        expiration_ts: &SystemTime,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let correlation_id = format!("{}.{}", key, child.correlation_id);
        let ttl = expiration_ts
            .time_remaining()
            .unwrap_or(Duration::default());
        let child_ref: object::DescriptorRef = child.into();
        shared::amqp::publish_job_request(
            &self.channel,
            child_ref,
            ttl,
            Some(correlation_id),
            Some(&self.queue),
        )
        .await
        .map(|_| ())
    }

    /// Retrieves and returns the first available child job result
    ///
    /// Note: a [`None`] result indicates a fatal condition
    async fn get_child_result(&mut self) -> Option<(String, String, JobResult)> {
        loop {
            debug!("Awaiting child job results...");
            // Receive broker message
            let msg = match self.subscription.recv().await {
                Some(m) => m,
                None => {
                    error!("Failed to receive job result: channel closed by server");
                    return None;
                }
            };

            let deliver = msg.deliver.as_ref().unwrap();
            let delivery_tag = deliver.delivery_tag();
            let bprops = msg.basic_properties.as_ref().unwrap();

            debug!(
                "Received job result message with delivery_tag={}\n",
                delivery_tag
            );

            // Sanitize message
            // If not well formed the message will be ignored (queue is auto-ack)
            match bprops.message_type() {
                Some(v) if v == shared::RESULT_TYPE => {}
                _ => {
                    warn!("Job result message has missing or invalid type");
                    continue;
                }
            };
            match bprops.content_type() {
                Some(v) if v == shared::MSG_CONTENT_TYPE => {}
                _ => {
                    warn!("Job result message has missing or invalid content_type");
                    continue;
                }
            }
            let correlation_id = match bprops.correlation_id() {
                Some(v)
                    if v.len() == shared::MSG_CORRID_LEN * 2 + 1
                        && v.is_ascii()
                        && v.chars().nth(shared::MSG_CORRID_LEN) == Some('.') =>
                {
                    v
                }
                _ => {
                    warn!("Job result message has missing or invalid correlation_id");
                    continue;
                }
            };
            let object: JobResult = match serde_json::from_slice(msg.content.as_ref().unwrap()) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Job result message has invalid payload: {}", e);
                    continue;
                }
            };
            let key = correlation_id
                .get(0..shared::MSG_CORRID_LEN)
                .unwrap()
                .to_string();
            let subkey = correlation_id
                .get((shared::MSG_CORRID_LEN + 1)..)
                .unwrap()
                .to_string();
            debug!("Child job result received: {:#?}", object);
            return Some((key, subkey, object));
        }
    }

    /// Returns a fully completed job, possibly awaiting for child results
    ///
    /// Note: a [`None`] result indicates a fatal condition
    async fn get_complete_job(&mut self) -> Option<PendingResult> {
        loop {
            let ready = self
                .jobs
                .iter()
                .find(|&(_, job)| job.is_complete() || job.is_expired())
                .map(|(k, _)| k.clone());
            if let Some(key) = ready {
                let job = self.jobs.remove(&key)?;
                if job.is_complete() {
                    info!(
                        "Job for object \"{}\" is complete",
                        job.result.info.object_id
                    );
                } else {
                    warn!(
                        "Job for object \"{}\" expired while awaiting child results",
                        job.result.info.object_id
                    );
                    metrics::job_timed_out();
                }
                metrics::set_waiting_count(self.jobs.len());
                return Some(job);
            }

            // Wait for the next completed job but only up to the next expiry, if any
            let next_expiry: Option<Duration> = self
                .jobs
                .values()
                .map(|j| j.time_remaining().unwrap_or(Duration::ZERO))
                .min();
            if let Some(wait) = next_expiry {
                let wait = wait + Duration::from_secs_f64(1.5); // Give results some small extra time
                let res = tokio::time::timeout(wait, self.await_completed_job()).await;
                if let Ok(res) = res {
                    // Some job completed
                    return res;
                } else {
                    // Time out: some job expired
                    continue;
                }
            } else {
                // No job present, no need for a timeout
                return self.await_completed_job().await;
            }
        }
    }

    async fn await_completed_job(&mut self) -> Option<PendingResult> {
        // Note: unknown keys are the result of duplicated messages and are
        // therefore silently ignored below
        'await_results: loop {
            let (key, subkey, mut result) = self.get_child_result().await?;
            if let Some(job) = self.jobs.get_mut(&key) {
                for child in job.children.iter_mut() {
                    if let PendingChildKind::Pending(pchild) = child {
                        if pchild.correlation_id == subkey {
                            info!(
                                "Received result for object \"{}\" (child of \"{}\")",
                                result.info.object_id, job.result.info.object_id
                            );
                            result.info.ctime = job.result.info.ctime;
                            *child = PendingChildKind::Complete(result);
                            if job.is_complete() {
                                debug!(
                                    "Result for object \"{}\" is now complete",
                                    job.result.info.object_id
                                );
                                let job = self.jobs.remove(&key);
                                metrics::set_waiting_count(self.jobs.len());
                                return job;
                            }
                            continue 'await_results;
                        }
                    }
                }
            }
        }
    }
}
