//! Message broker AMQP communication
//!
//! This module handles the reception of work results
use amqprs::{
    channel::{
        BasicAckArguments, BasicConsumeArguments, BasicPublishArguments, BasicQosArguments,
        BasicRejectArguments, Channel, QueueDeclareArguments,
    },
    connection::Connection,
    consumer::AsyncConsumer,
    BasicProperties, Deliver, FieldTable, FieldValue,
};
use shared::{
    amqp::{publish_job_request, request_apply_scenarios, JobResult},
    config::BrokerConfig,
};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

const WORK_COUNT: &str = "grapher_works_total";
const GRAPH_LATENCY: &str = "grapher_graphing_latency_seconds";

/// The interface to the AMQP broker
pub struct Broker {
    connection: Connection,
    channel: Channel,
    ctag: String,
}

impl Broker {
    /// Creates a new broker interface
    pub async fn new(
        broker_cfg: &BrokerConfig,
        graphdb: crate::graph::GraphDB,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Describe metrics
        metrics::describe_counter!(WORK_COUNT, "Total number of work requests processed");
        metrics::describe_histogram!(
            GRAPH_LATENCY,
            metrics::Unit::Seconds,
            "Time required to save a complete work result graph"
        );

        // Connect to broker
        let connection = shared::amqp::connect(broker_cfg).await?;

        // Create channel
        let channel = match shared::amqp::open_channel(&connection).await {
            Err(e) => {
                shared::amqp::close_connection(connection).await;
                return Err(e);
            }
            Ok(v) => v,
        };

        match async {
            // Declare director queue
            shared::amqp::declare_director_queue(&channel).await?;

            // Set qos = 1 message
            // channel
            //     .basic_qos(BasicQosArguments::new(0, 10, false))
            //     .await?;
            // Create or join the work result queue
            debug!(
                "Declaring the work result queue \"{}\"...",
                shared::RESULTS_QUEUE_NAME
            );
            let mut args = FieldTable::new();
            args.insert("x-queue-type".try_into().unwrap(), "quorum".into());
            args.insert(
                "x-message-ttl".try_into().unwrap(),
                FieldValue::i((shared::MAX_WORK_TTL * 2).as_millis().try_into().unwrap()),
            );
            let qargs = QueueDeclareArguments::new(shared::RESULTS_QUEUE_NAME)
                .durable(true)
                .arguments(args)
                .finish();

            let (_, message_count, consumer_count) = channel
                .queue_declare(qargs)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to declare the work result queue \"{}\": {}",
                        shared::RESULTS_QUEUE_NAME,
                        e
                    );
                    e
                })?
                .unwrap();
            debug!(
                "Work result queue \"{}\" successfully declared ({} messages, {} consumers)",
                shared::RESULTS_QUEUE_NAME,
                message_count,
                consumer_count
            );

            // Subscribe to the work result queue
            let args = BasicConsumeArguments::default()
                .queue(shared::RESULTS_QUEUE_NAME.to_owned())
                .auto_ack(false)
                .finish();
            let ctag = channel.basic_consume(graphdb, args).await.map_err(|e| {
                error!(
                    "Failed to subscribe to the work result queue \"{}\": {}",
                    shared::RESULTS_QUEUE_NAME,
                    e
                );
                e
            })?;
            debug!(
                "Subscribed to the work result queue \"{}\" with ctag {}",
                shared::RESULTS_QUEUE_NAME,
                ctag
            );
            Ok(ctag)
        }
        .await
        {
            Ok(ctag) => Ok(Self {
                connection,
                channel,
                ctag,
            }),
            Err(e) => {
                shared::amqp::close_channel(channel).await;
                shared::amqp::close_connection(connection).await;
                Err(e)
            }
        }
    }

    /// Cleanly unsubscribes, closes the channel and disconnects
    pub async fn close(self) {
        shared::amqp::unsubscribe(&self.channel, &self.ctag).await;
        shared::amqp::close_channel(self.channel).await;
        shared::amqp::close_connection(self.connection).await;
    }
}

#[async_trait::async_trait]
impl AsyncConsumer for crate::graph::GraphDB {
    /// Retrieves the first available work result from the queue
    ///
    /// It additionally validates the message, its properties and body; non conformant
    /// messages are ignored and rejected (i.e. left in the queue)
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        bprops: BasicProperties,
        content: Vec<u8>,
    ) {
        let delivery_tag = deliver.delivery_tag();
        debug!(
            "Received work result message with delivery_tag={}\n",
            delivery_tag
        );

        // Sanitize message
        // If not well formed the message will be rejected
        match bprops.message_type() {
            Some(v) if v == shared::RESULT_TYPE => {}
            _ => {
                warn!("Work result message has missing or invalid type");
                if reject_tag(channel, delivery_tag).await.is_err() {
                    self.notify_failure();
                }
                return;
            }
        };
        match bprops.content_type() {
            Some(v) if v == shared::MSG_CONTENT_TYPE => {}
            _ => {
                warn!("Work result message has missing or invalid content_type");
                if reject_tag(channel, delivery_tag).await.is_err() {
                    self.notify_failure();
                }
                return;
            }
        }
        let work_id = match bprops.correlation_id() {
            Some(v) if v.len() == shared::MSG_CORRID_LEN => v,
            _ => {
                warn!("Work result message has missing or invalid correlation_id");
                if reject_tag(channel, delivery_tag).await.is_err() {
                    self.notify_failure();
                }
                return;
            }
        };
        let work: JobResult = match serde_json::from_slice(&content) {
            Ok(v) => v,
            Err(e) => {
                warn!("Work result message has invalid payload: {}", e);
                if reject_tag(channel, delivery_tag).await.is_err() {
                    self.notify_failure();
                }
                return;
            }
        };
        debug!("Work result received: {:#?}", work);

        // Handle reprocessing for decryption in case the work:
        // - is reprocessable
        // - contains at least one object which failed to decrypt
        // - contains at least one possible password
        if let Some(serde_json::Value::Bool(true)) =
            work.relation_metadata.get(shared::META_KEY_REPROCESSABLE)
        {
            if work
                .walk()
                .any(|res| res.has_symbol("ENCRYPTED") && !res.has_symbol("DECRYPTED"))
            {
                let ppwds = work.get_possible_passwords();
                if !ppwds.is_empty() {
                    let ppwds = serde_json::Value::Array(
                        ppwds
                            .into_iter()
                            .map(|p| serde_json::Value::String(p.to_string()))
                            .collect(),
                    );
                    let mut relation_metadata = work.relation_metadata;
                    let mut max_recursion: Option<u32> = None;
                    let mut ttl: Option<std::time::Duration> = None;
                    if let Some(serde_json::Value::Object(map)) =
                        relation_metadata.get(shared::META_KEY_ORIGIN)
                    {
                        if let Some(serde_json::Value::Number(v)) = map.get("max_recursion") {
                            if let Some(v) = v.as_u64() {
                                if let Ok(v) = u32::try_from(v) {
                                    max_recursion = Some(v);
                                }
                            }
                        }
                        if let Some(serde_json::Value::Number(v)) = map.get("ttl") {
                            if let Some(v) = v.as_u64() {
                                ttl = Some(std::time::Duration::from_secs(v));
                            }
                        }
                    }
                    relation_metadata
                        .insert(shared::META_KEY_REPROCESSABLE.to_string(), false.into());
                    let mut globs = if let Some(serde_json::Value::Object(map)) =
                        relation_metadata.remove(shared::META_KEY_GLOBAL)
                    {
                        map
                    } else {
                        serde_json::Map::new()
                    };
                    globs.insert("possible_passwords".to_string(), ppwds);
                    relation_metadata.insert(
                        shared::META_KEY_GLOBAL.to_string(),
                        serde_json::Value::Object(globs),
                    );
                    let desc = shared::object::Descriptor {
                        info: work.info,
                        symbols: Vec::new(),
                        relation_metadata,
                        max_recursion: max_recursion
                            .unwrap_or(shared::MAX_WORK_DEPTH)
                            .min(shared::MAX_WORK_DEPTH),
                    };
                    info!("Reprocessing work \"{}\" for decryption", work_id);
                    let resubmit_ok = publish_job_request(
                        channel,
                        (&desc).into(),
                        ttl.unwrap_or(shared::MAX_WORK_TTL)
                            .min(shared::MAX_WORK_TTL),
                        Some(work_id.to_string()),
                        None,
                    )
                    .await
                    .map_err(|e| {
                        warn!(
                            "Failed to publish reprocess request forfor \"{}\": {}",
                            work_id, e
                        );
                        e
                    })
                    .is_ok();
                    if resubmit_ok {
                        if let Err(e) = ack_tag(channel, delivery_tag).await {
                            error!("Broker error, exiting: {}", e);
                            return;
                        }
                    } else {
                        if reject_tag(channel, delivery_tag).await.is_err() {
                            self.notify_failure();
                        }
                    }
                    return;
                }
            }
        }

        // Save work result
        metrics::counter!(WORK_COUNT).increment(1);
        let now = std::time::Instant::now();
        info!(
            "Received work result for \"{}\" with delivery_tag {}",
            work_id, delivery_tag
        );
        if let Err(e) = self.save_result(work, work_id.to_string()).await {
            error!("Graph database error, exiting: {}", e);
            reject_tag(channel, delivery_tag).await.ok();
            self.notify_failure();
            return;
        }
        if let Err(e) = ack_tag(channel, delivery_tag).await {
            error!("Broker error, exiting: {}", e);
            self.notify_failure();
            return;
        }
        metrics::histogram!(GRAPH_LATENCY).record(now.elapsed().as_secs_f64());

        // Notify the director about the newly completed work
        if let Err(e) = request_apply_scenarios(work_id.to_string(), channel).await {
            warn!(
                "Failed to publish process request for work \"{}\": {}",
                work_id, e
            );
        } else {
            debug!("Published process request for work \"{}\"", work_id);
        }
    }
}

/// Rejects a work result message which will be re-published later
///
/// Note: an [`Error`](std::error::Error) result indicates a fatal condition
async fn reject_tag(
    channel: &Channel,
    delivery_tag: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Sending reject for delivery_tag={}", delivery_tag);
    channel
        .basic_reject(BasicRejectArguments::new(delivery_tag, true))
        .await
        .map_err(|e| {
            error!(
                "Failed to reject work result with delivery tag {}: {}",
                delivery_tag, e
            );
            e
        })?;
    Ok(())
}

/// Acknowledges a work result message, removing it from the broker
///
/// Note: a [`Error`](std::error::Error) result indicates a fatal condition
async fn ack_tag(channel: &Channel, delivery_tag: u64) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Sending ack for delivery_tag={}", delivery_tag);
    channel
        .basic_ack(BasicAckArguments::new(delivery_tag, false))
        .await
        .map_err(|e| {
            error!(
                "Failed to ack job result with delivery tag {}: {}",
                delivery_tag, e
            );
            e
        })?;
    Ok(())
}
