use actix_web::{error, Result, HttpResponse, web::{Data, Json}};
use fs_protobuf_rust::compiled::mcfs;
use serde::{Deserialize, Serialize};

use crate::{flight::FlightComputer, Database};

/// Request struct containing all necessary information to execute a command.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OperatorCommandRequest {
	command: String,
	target: Option<String>,
	state: Option<String>,
}

/// Route handler to dispatch a single manual operator command
pub async fn dispatch_operator_command(
	request: Json<OperatorCommandRequest>,
	flight_computer: Data<FlightComputer>,
	database: Data<Database>,
) -> Result<HttpResponse> {
	if !flight_computer.is_connected().await {
		return Err(error::ErrorInternalServerError("flight computer not connected"));
	}

	let mut node_id = None;

	if let Some(target) = &request.target {
		use mcfs::device::Channel;

		node_id = Some(
			database
				.lock()
				.await
				.query_row(
					"SELECT node_id FROM NodeMappings WHERE text_identifier = ?1",
					rusqlite::params![target],
					|row| Ok(mcfs::device::NodeIdentifier {
						board_id: row.get(0)?,
						channel: Channel::from(row.get::<_, i32>(1)?),
						node_id: row.get(2)?,
					})
				)
				.map_err(|_| error::ErrorBadRequest("target identifier not found"))?
		);
	}

	let command = match request.command.as_str() {
		"click_valve" => {
			if let Some(node_id) = &node_id {
				if node_id.channel != mcfs::device::Channel::VALVE {
					return Err(error::ErrorBadRequest("target is not a valve"));
				}
			} else {
				return Err(error::ErrorBadRequest("target valve is required"));
			}

			let state = match request.state.as_deref() {
				Some("open") => mcfs::device::ValveState::VALVE_OPEN,
				Some("closed") => mcfs::device::ValveState::VALVE_CLOSED,
				None => Err(error::ErrorBadRequest("valve state is required"))?,
				_ => Err(error::ErrorBadRequest("unrecognized state identifier"))?,
			};

			mcfs::command::mod_Command::OneOfcommand::click_valve(
				mcfs::command::ClickValve {
					valve: node_id,
					state,
				}
			)
		},
		"set_led" => {
			if let Some(node_id) = &node_id {
				if node_id.channel != mcfs::device::Channel::LED {
					return Err(error::ErrorBadRequest("target is not an LED"));
				}
			} else {
				return Err(error::ErrorBadRequest("target LED is required"));
			}

			let state = match request.state.as_deref() {
				Some("on") => mcfs::device::LEDState::LED_ON,
				Some("off") => mcfs::device::LEDState::LED_OFF,
				None => Err(error::ErrorBadRequest("state field required"))?,
				_ => Err(error::ErrorBadRequest("unrecognized state identifier"))?,
			};

			mcfs::command::mod_Command::OneOfcommand::set_led(
				mcfs::command::SetLED {
					led: node_id,
					state,
				}
			)
		},
		_ => return Err(error::ErrorBadRequest("unrecognized command identifier")),
	};

	let message = quick_protobuf::serialize_into_vec(
		&mcfs::core::Message {
			timestamp: None,
			board_id: 32,
			content: mcfs::core::mod_Message::OneOfcontent::command(
				mcfs::command::Command { command }
			)
		}
	).map_err(|_| error::ErrorInternalServerError("failed to parse message into protobuf"))?;

	flight_computer.send_bytes(&message)
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send message to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}
