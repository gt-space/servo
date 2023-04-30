use actix_web::{error, Result, HttpResponse, web::Data};
use fs_protobuf_rust::compiled::mcfs;

use crate::flight::FlightComputer;

pub async fn set_led(flight_computer: Data<FlightComputer>) -> Result<HttpResponse> {
	if !flight_computer.is_connected().await {
		return Err(error::ErrorInternalServerError("flight computer not connected"));
	}

	let message = quick_protobuf::serialize_into_vec(
		&mcfs::core::Message {
			timestamp: None,
			board_id: 150,
			content: mcfs::core::mod_Message::OneOfcontent::command(
				mcfs::command::Command {
					command: mcfs::command::mod_Command::OneOfcommand::set_led(
						mcfs::command::SetLED {
							led: Some(mcfs::device::NodeIdentifier {
								board_id: 1,
								channel: mcfs::device::Channel::LED,
								node_id: 1,
							}),
							state: mcfs::device::LEDState::LED_ON
						}
					)
				}
			)
		}
	).map_err(|_| error::ErrorInternalServerError("failed to construct protobuf message"))?;

	flight_computer.send_bytes(&message)
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send message to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}