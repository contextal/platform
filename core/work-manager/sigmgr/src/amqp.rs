//! Message broker AMQP communication
//!
//! This module handles the reception of work results
use amqprs::{
    channel::{BasicConsumeArguments, Channel},
    connection::Connection,
    consumer::AsyncConsumer,
    BasicProperties, Deliver,
};
use shared::{self, config::BrokerConfig};
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The interface to the AMQP broker
pub struct Broker {
    connection: Connection,
    reload_channel: Channel,
    reload_ctag: String,
}

impl Broker {
    /// Creates a new broker interface
    pub async fn new(
        broker_cfg: &BrokerConfig,
        newrules_notice: Arc<tokio::sync::Notify>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Connect to broker
        let connection = shared::amqp::connect(broker_cfg).await?;

        // Create reload channel
        let reload_channel = match shared::amqp::open_channel(&connection).await {
            Err(e) => {
                shared::amqp::close_connection(connection).await;
                return Err(e);
            }
            Ok(v) => v,
        };

        let reload_ctag = match async {
            // Declare reload exchange and queue
            let reload_queue = shared::amqp::declare_reload_queue(&reload_channel).await?;

            // Subscribe to the reload queue
            let args = BasicConsumeArguments::new(&reload_queue, "")
                .auto_ack(true)
                .exclusive(true)
                .finish();
            let reload_ctag = reload_channel
                .basic_consume(ReloadNotifier(newrules_notice), args)
                .await
                .inspect_err(|e| {
                    error!(
                        "Failed to subscribe to the reload queue \"{}\": {}",
                        reload_queue, e
                    )
                })?;
            debug!(
                "Subscribed to the reload queue \"{}\" with ctag {}",
                reload_queue, reload_ctag
            );
            Ok(reload_ctag)
        }
        .await
        {
            Ok(v) => v,
            Err(e) => {
                shared::amqp::close_channel(reload_channel).await;
                shared::amqp::close_connection(connection).await;
                return Err(e);
            }
        };
        Ok(Self {
            connection,
            reload_channel,
            reload_ctag,
        })
    }

    /// Cleanly unsubscribes, closes the channel and disconnects
    pub async fn close(self) {
        shared::amqp::unsubscribe(&self.reload_channel, &self.reload_ctag).await;
        shared::amqp::close_channel(self.reload_channel).await;
        shared::amqp::close_connection(self.connection).await;
    }
}

struct ReloadNotifier(Arc<tokio::sync::Notify>);

#[async_trait::async_trait]
impl AsyncConsumer for ReloadNotifier {
    /// Handles scenarios reload requests
    async fn consume(
        &mut self,
        _channel: &Channel,
        deliver: Deliver,
        bprops: BasicProperties,
        _content: Vec<u8>,
    ) {
        let delivery_tag = deliver.delivery_tag();
        match bprops.message_type() {
            Some(v) if v == shared::SC_RELOAD_TYPE => {
                debug!(
                    "Received scenario reload request message with delivery_tag={}\n",
                    delivery_tag
                );
                self.0.notify_one();
            }
            _ => {
                warn!(
                    "Reload request message has missing or invalid type (delivery_tag = {})",
                    delivery_tag
                );
            }
        }
    }
}
