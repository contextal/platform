//! Message broker AMQP communication
//!
//! This module handles the publication of job requests
use amqprs::{
    channel::{BasicPublishArguments, Channel, QueueDeclareArguments},
    connection::Connection,
    BasicProperties, FieldTable, FieldValue,
};
use shared::{config::BrokerConfig, object};
use std::time::Duration;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The interface to the AMQP broker
pub struct Broker {
    connection: Connection,
    channel: Channel,
}

impl Broker {
    /// Creates a new broker interface
    pub async fn new(broker_cfg: &BrokerConfig) -> Result<Self, Box<dyn std::error::Error>> {
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
        let (_, message_count, consumer_count) = match channel.queue_declare(qargs).await {
            Err(e) => {
                error!(
                    "Failed to declare the work result queue \"{}\": {}",
                    shared::RESULTS_QUEUE_NAME,
                    e
                );
                shared::amqp::close_channel(channel).await;
                shared::amqp::close_connection(connection).await;
                return Err(e.into());
            }
            Ok(v) => v.unwrap(),
        };
        debug!(
            "Work result queue \"{}\" successfully declared ({} messages, {} consumers)",
            shared::RESULTS_QUEUE_NAME,
            message_count,
            consumer_count
        );

        Ok(Self {
            connection,
            channel,
        })
    }

    /// Cleanly closes the channel and disconnects
    pub async fn close(self) {
        shared::amqp::close_channel(self.channel).await;
        shared::amqp::close_connection(self.connection).await;
    }

    /// Posts a job request
    pub async fn publish_job_request(
        &self,
        object: object::Info,
        ttl: Duration,
        max_recursion: u32,
        relation_metadata: object::Metadata,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let symbols: Vec<String> = Vec::new();
        let req_object = object::DescriptorRef {
            info: &object,
            symbols: &symbols,
            relation_metadata: &relation_metadata,
            max_recursion,
        };
        shared::amqp::publish_job_request(&self.channel, req_object, ttl, None, None).await
    }

    /// Issue a scenario reload request
    pub async fn request_reload_scenarios(&self) -> Result<(), Box<dyn std::error::Error>> {
        let bprops = BasicProperties::default()
            .with_message_type(shared::SC_RELOAD_TYPE)
            .finish();
        let args = BasicPublishArguments::new(shared::SC_RELOAD_EXCHANGE_NAME, "");
        self.channel
            .basic_publish(bprops, Vec::new(), args)
            .await
            .map_err(|e| {
                error!(
                    "Failed to publish reload request to {}: {}",
                    shared::SC_RELOAD_EXCHANGE_NAME,
                    e
                );
                e
            })?;
        debug!(
            "Posted scenario reload request to {}",
            shared::SC_RELOAD_EXCHANGE_NAME
        );
        Ok(())
    }

    /// Issue apply scenario requests
    pub async fn request_apply_scenarios(
        &self,
        work_ids: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for work_id in work_ids {
            shared::amqp::request_apply_scenarios(work_id, &self.channel)
                .await
                .map_err(|e| {
                    error!("Failed to publish apply scenarios request: {}", e);
                    e
                })?;
        }
        Ok(())
    }
}
