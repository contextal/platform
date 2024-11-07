//! Message broker AMQP communication
//!
//! This module handles the reception of work results
use amqprs::{
    channel::{
        BasicAckArguments, BasicConsumeArguments, BasicRejectArguments, Channel, ConsumerMessage,
        ExchangeDeclareArguments, ExchangeType, QueueBindArguments, QueueDeclareArguments,
    },
    connection::Connection,
};
use shared::{self, config::BrokerConfig, scene};
use tokio::sync::mpsc::UnboundedReceiver;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The interface to the AMQP broker
pub struct Broker {
    connection: Connection,
    reload_channel: Channel,
    reload_ctag: String,
    reload_receiver: UnboundedReceiver<ConsumerMessage>,
    director_channel: Channel,
    director_ctag: String,
    director_receiver: UnboundedReceiver<ConsumerMessage>,
    graphdb: crate::graph::GraphDB,
    reload_requested: bool,
}

impl Broker {
    /// Creates a new broker interface
    pub async fn new(
        broker_cfg: &BrokerConfig,
        graphdb: crate::graph::GraphDB,
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

        let (reload_ctag, reload_receiver) = match async {
            // Declare reload exchange and queue
            debug!(
                "Declaring the reload exchange \"{}\"...",
                shared::SC_RELOAD_EXCHANGE_NAME
            );
            reload_channel
                .exchange_declare(
                    ExchangeDeclareArguments::of_type(
                        shared::SC_RELOAD_EXCHANGE_NAME,
                        ExchangeType::Fanout,
                    )
                    .durable(true)
                    .finish(),
                )
                .await
                .map_err(|e| {
                    error!("Failed to declare the reload exchange: {}", e);
                    e
                })?;
            let qargs = QueueDeclareArguments::default()
                .exclusive(true)
                .auto_delete(true)
                .finish();
            let (reload_queue, message_count, consumer_count) = reload_channel
                .queue_declare(qargs)
                .await
                .map_err(|e| {
                    error!("Failed to declare the reload queue: {}", e);
                    e
                })?
                .unwrap();
            debug!(
                "Reload queue \"{}\" successfully declared ({} messages, {} consumers)",
                reload_queue, message_count, consumer_count
            );

            // Bind the reload queue to the reload exchange
            reload_channel
                .queue_bind(QueueBindArguments::new(
                    &reload_queue,
                    shared::SC_RELOAD_EXCHANGE_NAME,
                    "",
                ))
                .await
                .map_err(|e| {
                    error!("Failed to bind the reload queue to its exchange: {}", e);
                    e
                })?;
            debug!(
                "Reload queue \"{}\" declared ({} messages, {} consumers) and bound to {}",
                reload_queue,
                message_count,
                consumer_count,
                shared::SC_RELOAD_EXCHANGE_NAME
            );

            // Subscribe to the reload queue
            let args = BasicConsumeArguments::new(&reload_queue, "")
                .auto_ack(true)
                .exclusive(true)
                .finish();
            let (reload_ctag, reload_receiver) =
                reload_channel.basic_consume_rx(args).await.map_err(|e| {
                    error!(
                        "Failed to subscribe to the reload queue \"{}\": {}",
                        reload_queue, e
                    );
                    e
                })?;
            debug!(
                "Subscribed to the reload queue \"{}\" with ctag {}",
                reload_queue, reload_ctag
            );
            Ok((reload_ctag, reload_receiver))
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

        // Create director channel
        let director_channel = match shared::amqp::open_channel(&connection).await {
            Err(e) => {
                shared::amqp::unsubscribe(&reload_channel, &reload_ctag).await;
                shared::amqp::close_channel(reload_channel).await;
                shared::amqp::close_connection(connection).await;
                return Err(e);
            }
            Ok(v) => v,
        };

        let (director_ctag, director_receiver) = match async {
            // Declare director queue
            shared::amqp::declare_director_queue(&director_channel).await?;

            // Subscribe to the director queue
            let args = BasicConsumeArguments::default()
                .queue(shared::DIRECTOR_QUEUE_NAME.to_owned())
                .auto_ack(false)
                .finish();
            let (director_ctag, director_receiver) =
                director_channel.basic_consume_rx(args).await.map_err(|e| {
                    error!(
                        "Failed to subscribe to the director queue \"{}\": {}",
                        shared::DIRECTOR_QUEUE_NAME,
                        e
                    );
                    e
                })?;
            debug!(
                "Subscribed to the director queue \"{}\" with ctag {}",
                shared::DIRECTOR_QUEUE_NAME,
                director_ctag
            );
            Ok((director_ctag, director_receiver))
        }
        .await
        {
            Ok(v) => v,
            Err(e) => {
                shared::amqp::close_channel(director_channel).await;
                shared::amqp::unsubscribe(&reload_channel, &reload_ctag).await;
                shared::amqp::close_channel(reload_channel).await;
                shared::amqp::close_connection(connection).await;
                return Err(e);
            }
        };
        Ok(Self {
            connection,
            reload_channel,
            reload_ctag,
            reload_receiver,
            director_channel,
            director_ctag,
            director_receiver,
            graphdb,
            reload_requested: false,
        })
    }

    /// Cleanly unsubscribes, closes the channel and disconnects
    pub async fn close(self) {
        shared::amqp::unsubscribe(&self.director_channel, &self.director_ctag).await;
        shared::amqp::close_channel(self.director_channel).await;
        shared::amqp::unsubscribe(&self.reload_channel, &self.reload_ctag).await;
        shared::amqp::close_channel(self.reload_channel).await;
        shared::amqp::close_connection(self.connection).await;
    }

    /// Process director requests
    ///
    /// Note: never returns except on fatal errors
    pub async fn process_requests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        async fn make_interval(rng: &mut rand::rngs::ThreadRng) -> tokio::time::Interval {
            use rand::Rng;
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(
                rng.gen_range(3000..6000),
            ));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await; // ticks immediately
            debug!("Reload interval set to {}", interval.period().as_millis());
            interval
        }
        let mut rng = rand::thread_rng();
        let mut interval = make_interval(&mut rng).await;
        loop {
            tokio::select!(
                msg = self.director_receiver.recv() => {
                    if let Some(msg) = msg {
                        self.process_apply(msg).await?;
                    } else {
                        break;
                    }
                }
                msg = self.reload_receiver.recv() => {
                    if let Some(msg) = msg {
                        self.schedule_reload(msg).await?;
                    } else {
                        break;
                    }
                },
                _ = interval.tick() => {
                    if self.reload_requested {
                        self.graphdb.load_scenarios().await?;
                        self.reload_requested = false;
                        interval = make_interval(&mut rng).await;
                    }
                }
            );
        }
        return Err("Broker lost".into());
    }

    /// Handles apply scenarios messages
    async fn process_apply(&self, msg: ConsumerMessage) -> Result<(), Box<dyn std::error::Error>> {
        // Note: unwaps on msg are safe - see amqprs docs
        let delivery_tag = msg.deliver.unwrap().delivery_tag();
        debug!(
            "Received processing request message with delivery_tag={}\n",
            delivery_tag
        );

        let bprops = msg.basic_properties.unwrap();
        match bprops.message_type() {
            Some(v) if v == shared::SC_PROCESS_TYPE => {}
            _ => {
                warn!("Processing request message has missing or invalid type");
                return self.reject_tag(delivery_tag).await;
            }
        };
        match bprops.content_type() {
            Some(v) if v == shared::MSG_CONTENT_TYPE => {}
            _ => {
                warn!("Processing request message has missing or invalid content_type");
                return self.reject_tag(delivery_tag).await;
            }
        }
        let request: scene::DirectorRequest = match serde_json::from_slice(&msg.content.unwrap()) {
            Ok(v) => v,
            Err(e) => {
                warn!("Processing request message has invalid payload: {}", e);
                return self.reject_tag(delivery_tag).await;
            }
        };
        debug!("Processing request received: {:#?}", request);
        if let Err(e) = self.graphdb.apply_scenarios(&request.work_id).await {
            error!("Graph database error, exiting: {}", e);
            self.reject_tag(delivery_tag).await.ok();
            return Err(e);
        }
        if let Err(e) = self.ack_tag(delivery_tag).await {
            error!("Broker error, exiting: {}", e);
            return Err(e);
        }
        info!("Scenarios applied to {}", request.work_id);
        Ok(())
    }

    /// Handles scenarios reload requests
    async fn schedule_reload(
        &mut self,
        msg: ConsumerMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Note: unwaps on msg are safe - see amqprs docs
        let delivery_tag = msg.deliver.unwrap().delivery_tag();
        debug!(
            "Received scenario reload request message with delivery_tag={}\n",
            delivery_tag
        );

        let bprops = msg.basic_properties.unwrap();
        match bprops.message_type() {
            Some(v) if v == shared::SC_RELOAD_TYPE => {
                self.reload_requested = true;
            }
            _ => {
                warn!("Reload request message has missing or invalid type");
            }
        };
        Ok(())
    }

    /// Rejects a processing request message
    ///
    /// Note: an [`Error`](std::error::Error) result indicates a fatal condition
    async fn reject_tag(&self, delivery_tag: u64) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending reject for delivery_tag={}", delivery_tag);
        self.director_channel
            .basic_reject(BasicRejectArguments::new(delivery_tag, true))
            .await
            .map_err(|e| {
                error!(
                    "Failed to reject processing request with delivery tag {}: {}",
                    delivery_tag, e
                );
                e
            })?;
        Ok(())
    }

    /// Acknowledges a processing request message, removing it from the broker
    ///
    /// Note: a [`Error`](std::error::Error) result indicates a fatal condition
    async fn ack_tag(&self, delivery_tag: u64) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Sending ack for delivery_tag={}", delivery_tag);
        self.director_channel
            .basic_ack(BasicAckArguments::new(delivery_tag, false))
            .await
            .map_err(|e| {
                error!(
                    "Failed to ack job processing request with delivery tag {}: {}",
                    delivery_tag, e
                );
                e
            })?;
        Ok(())
    }
}
