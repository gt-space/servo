use common::comm::{FlightControlMessage, NodeMapping, VehicleState, Sequence};
use jeflog::warn;
use super::{Database, SharedState};
use std::{future::Future, sync::Arc};
use tokio::{io::{self, AsyncWriteExt}, net::{TcpListener, TcpStream, UdpSocket}, sync::{Mutex, Notify}};

/// Struct capable of performing thread-safe operations on a flight computer
/// connection, thus capable of being passed to route handlers.
#[derive(Debug)]
pub struct FlightComputer {
	database: Database,
	stream: TcpStream,
	vehicle_state: Arc<(Mutex<VehicleState>, Notify)>,
	shared_self: Arc<(Mutex<Option<Self>>, Notify)>,
}

impl FlightComputer {
	/// A listener function which auto-connects to the flight computer.
	/// 
	/// The flight computer is expected to fetch the IP address of the
	/// ground computer by hostname resolution, outside the scope of servo.
	pub fn auto_connect(shared: &SharedState) -> impl Future<Output = io::Result<()>> {
		let database = shared.database.clone();
		let flight = shared.flight.clone();
		let vehicle_state = shared.vehicle.clone();

		async move {
			loop {
				let listener = TcpListener::bind("0.0.0.0:5025").await?;
				let (mut stream, _) = listener.accept().await?;
				let mut computer = flight.0.lock().await;

				if computer.is_none() {
					let flight = FlightComputer {
						stream,
						database: database.clone(),
						vehicle_state: vehicle_state.clone(),
						shared_self: flight.clone(),
					};

					tokio::spawn(flight.receive_vehicle_state());
					*computer = Some(flight);
				} else {
					stream.shutdown().await?;
				}
			}
		}
	}

	/// Send a slice of bytes along the TCP connection to the flight computer.
	pub async fn send_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
		self.stream.write_all(bytes).await
	}

	/// Sends the given set of mappings to the flight computer.
	pub async fn send_mappings(&mut self) -> anyhow::Result<()> {
		let mappings = self.database
			.connection
			.lock()
			.await
			.prepare("
				SELECT
					text_id,
					board_id,
					sensor_type,
					channel,
					computer,
					max,
					min,
					calibrated_offset,
					powered_threshold,
					normally_closed
				FROM NodeMappings WHERE active = TRUE
			")?
			.query_and_then([], |row| {
				Ok(NodeMapping {
					text_id: row.get(0)?,
					board_id: row.get(1)?,
					sensor_type: row.get(2)?,
					channel: row.get(3)?,
					computer: row.get(4)?,
					max: row.get(5)?,
					min: row.get(6)?,
					calibrated_offset: row.get(7)?,
					powered_threshold: row.get(8)?,
					normally_closed: row.get(9)?,
				})
			})?
			.collect::<Result<Vec<NodeMapping>, rusqlite::Error>>()?;

		let message = FlightControlMessage::Mappings(mappings);
		let serialized = postcard::to_allocvec(&message)?;

		self.send_bytes(&serialized).await?;

		Ok(())
	}

	/// Sends the given sequence to the flight computer to be executed.
	pub async fn send_sequence(&mut self, sequence: Sequence) -> anyhow::Result<()> {
		let message = FlightControlMessage::Sequence(sequence);
		let serialized = postcard::to_allocvec(&message)?;

		self.send_bytes(&serialized).await?;
		Ok(())
	}

	/// Repeatedly receives vehicle state information from the flight computer.
	pub fn receive_vehicle_state(&self) -> impl Future<Output = io::Result<()>> {
		let vehicle_state = self.vehicle_state.clone();
		let shared_self = self.shared_self.clone();

		async move {
			let socket = UdpSocket::bind("0.0.0.0:7201").await.unwrap();
			let mut frame_buffer = vec![0; 20_000];

			loop {
				match socket.recv_from(&mut frame_buffer).await {
					Ok((datagram_size, _)) => {
						if datagram_size == 0 {
							// if the datagram size is zero, the connection has been closed
							break;
						} else if datagram_size == frame_buffer.len() {
							frame_buffer.resize(frame_buffer.len() * 2, 0);
							println!("resized buffer");
							continue;
						}

						let new_state = postcard::from_bytes::<VehicleState>(&frame_buffer[..datagram_size]);

						match new_state {
							Ok(state) => {
								*vehicle_state.0.lock().await = state;
								vehicle_state.1.notify_waiters();
							},
							Err(error) => warn!("Failed to deserialize vehicle state: {error}"),
						};
					},
					Err(error) => {
						// Windows throws this error when the buffer is not large enough.
						// Unix systems just log whatever they can.
						if error.raw_os_error() == Some(10040) {
							frame_buffer.resize(frame_buffer.len() * 2, 0);
							continue;
						}

						break;
					}
				}
			}

			*shared_self.0.lock().await = None;
			shared_self.1.notify_waiters();

			Ok(())
		}
	}
}
