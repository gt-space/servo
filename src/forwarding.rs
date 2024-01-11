use actix_web::web;
use common::VehicleState;
use jeflog::warn;
use std::{future::Future, sync::Arc, time::Duration, io};
use tokio::{sync::{broadcast, Mutex, Notify}, time::MissedTickBehavior};
use tokio_stream::wrappers::BroadcastStream;

/// Agent which streams vehicle state to forwarding targets.
#[derive(Clone, Debug)]
pub struct ForwardingAgent {
	tx: broadcast::Sender<web::Bytes>,
	vehicle_state: Arc<(Mutex<VehicleState>, Notify)>,
}

impl ForwardingAgent {
	/// Creates new `ForwardingAgent` given a reference to the vehicle state tuple.
	pub fn new(vehicle_state: Arc<(Mutex<VehicleState>, Notify)>) -> Arc<Self> {
		let (tx, _) = broadcast::channel(10);

		Arc::new(ForwardingAgent { tx, vehicle_state })
	}

	/// Continuously forwards vehicle state in JSON format out to the streams requested by HTTP.
	pub fn forward(self: &Arc<Self>) -> impl Future<Output = io::Result<()>> {
		let weak_self = Arc::downgrade(self);

		async move {
			let mut interval = tokio::time::interval(Duration::from_millis(100));
			interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

			let mut buffer = Vec::with_capacity(20_000);

			while let Some(strong_self) = weak_self.upgrade() {
				interval.tick().await;

				if strong_self.tx.receiver_count() > 0 {
					let updated_state = strong_self.vehicle_state.0.lock().await;

					serde_json::to_writer(&mut buffer, &*updated_state)?;

					// theoretically, this should never error because according to the current docs,
					// a `SendError` only occurs when there are no receiving channels, but we have
					// already checked for that. if an error does occur here, it doesn't really matter
					// anyway. still, I have it printing a warning message; unexpected things can occur.

					if strong_self.tx.send(web::Bytes::copy_from_slice(&buffer)).is_err() {
						warn!("Broadcasting vehicle state to forwarding streams failed.");
					}

					buffer.clear();
				}
			}

			Ok(())
		}
	}

	/// Constructs a new `BroadcastStream` that is subscribed to the `ForwardingAgent` sender.
	pub fn stream(&self) -> BroadcastStream<web::Bytes> {
		BroadcastStream::from(self.tx.subscribe())
	}
}


