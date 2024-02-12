use actix_web::{error, Result, HttpResponse, web::{Data, Json}};
use common::comm::Sequence;
use serde::{Deserialize, Serialize};

use crate::{error::internal, flight::FlightComputer};

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
) -> Result<HttpResponse> {
	if !flight_computer.is_connected().await {
		return Err(error::ErrorInternalServerError("flight computer not connected"));
	}

	let command = match request.command.as_str() {
		"click_valve" => {
			let target = request.target.clone().ok_or(error::ErrorBadRequest("must supply target name"))?;

			let script = match request.state.as_deref() {
				Some("open") => format!("{target}.open()"),
				Some("closed") => format!("{target}.close()"),
				None => Err(error::ErrorBadRequest("valve state is required"))?,
				_ => Err(error::ErrorBadRequest("unrecognized state identifier"))?,
			};

			common::comm::FlightControlMessage::Sequence(Sequence { name: "command".to_owned(), script })
		},
		_ => return Err(error::ErrorBadRequest("unrecognized command identifier")),
	};

	let serialized = postcard::to_allocvec(&command)
		.map_err(internal)?;

	flight_computer.send_bytes(&serialized)
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send message to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}
