use tokio::{sync::Mutex, io::{self, AsyncWriteExt}, net::TcpStream};
use std::{future::Future, sync::Arc, net::{Ipv4Addr, SocketAddr, SocketAddrV4}, time::Duration};

use crate::routes::mappings::NodeMapping;

/// Struct capable of performing thread-safe operations on a flight computer
/// connection, thus capable of being passed to route handlers.
#[derive(Clone, Debug)]
pub struct FlightComputer {
	connection: Arc<Mutex<Option<TcpStream>>>
}

impl FlightComputer {
	/// Constructs a thread-safe wrapper around a TCP stream which can be
	/// connected to the flight computer.
	pub fn new() -> Self {
		FlightComputer { connection: Arc::new(Mutex::new(None)) }
	}

	/// A getter which determines whether the flight computer is connected.
	pub async fn is_connected(&self) -> bool {
		return self.connection
			.lock()
			.await
			.is_some();
	}

	/// A listener function which auto-connects to the flight computer.
	/// 
	/// The flight computer is expected to fetch the IP address of the
	/// ground computer by hostname resolution, outside the scope of servo.
	pub fn auto_connect(&self) -> impl Future<Output = io::Result<()>> {
		let connection = self.connection.clone();

		async {
			let flight_listener = std::net::TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5025)))?;
			flight_listener.set_nonblocking(true)?;

			let flight_stream = loop {
				if let Ok((stream, _)) = flight_listener.accept() {
					break stream;
				}

				tokio::time::sleep(Duration::from_millis(500)).await;
			};

			*connection.lock_owned().await = Some(TcpStream::from_std(flight_stream)?);
			println!("connected to the flight computer");
			Ok(())
		}
	}

	/// Send a slice of bytes along the TCP connection to the flight computer.
	pub async fn send_bytes(&self, bytes: &[u8]) -> io::Result<()> {
		self.connection
			.lock()
			.await
			.as_mut()
			.ok_or(io::Error::new(io::ErrorKind::NotConnected, "flight computer not connected"))?
			.write_all(bytes)
			.await?;

		Ok(())
	}

	/// Sends the given set of mappings to the flight computer.
	pub async fn send_mappings<'a, I>(&self, mappings: I) -> io::Result<()> where I: IntoIterator<Item = &'a NodeMapping> {
		use fs_protobuf_rust::compiled::mcfs::{
			core::{mod_Message, Message},
			mapping::ChannelMapping,
			board::{ChannelIdentifier, ChannelType},
		};

		let mappings = fs_protobuf_rust::compiled::mcfs::mapping::Mapping {
			channel_mappings: mappings
				.into_iter()
				.map(|mapping| {
					let channel_type = match mapping.channel_type.as_ref() {
						"gpio" => ChannelType::GPIO,
						"led" => ChannelType::LED,
						"rail_3v3" => ChannelType::RAIL_3V3,
						"rail_5v" => ChannelType::RAIL_5V,
						"rail_5v5" => ChannelType::RAIL_5V5,
						"rail_24v" => ChannelType::RAIL_24V,
						"current_loop" => ChannelType::CURRENT_LOOP,
						"differential_signal" => ChannelType::DIFFERENTIAL_SIGNAL,
						"tc" => ChannelType::TC,
						"valve_current" => ChannelType::VALVE_CURRENT,
						"valve_voltage" => ChannelType::VALVE_VOLTAGE,
						"rtd" => ChannelType::RTD,
						"valve" => ChannelType::VALVE,
						_ => panic!("invalid channel type"),
					};
			
					ChannelMapping {
						name: mapping.text_id.clone().into(),
						channel_identifier: Some(ChannelIdentifier {
							board_id: mapping.board_id,
							channel_type,
							channel: mapping.channel,
						})
					}
				})
				.collect::<Vec<_>>()
		};

		let message = Message {
			timestamp: None,
			board_id: 0,
			content: mod_Message::OneOfcontent::mapping(mappings)
		};

		let serialized = quick_protobuf::serialize_into_vec(&message)
			.unwrap();

		self.send_bytes(&serialized).await?;

		Ok(())
	}

	/// Sends the given sequence to the flight computer to be executed.
	pub async fn send_sequence(&self, sequence: &str) -> io::Result<()> {
		use fs_protobuf_rust::compiled::mcfs::{
			core::{mod_Message, Message},
			sequence::Sequence
		};

		let message = Message {
			timestamp: None,
			board_id: 0,
			content: mod_Message::OneOfcontent::sequence(Sequence {
				name: "sequence".into(),
				script: sequence.into(),
			})
		};
		
		self.send_bytes(&quick_protobuf::serialize_into_vec(&message).unwrap()).await?;

		Ok(())
	}
}