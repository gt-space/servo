use axum::{extract::State, Json};
use common::comm::Sequence;
use crate::server::{self, Shared, error::{bad_request, internal}};
use serde::{Deserialize, Serialize};

/// Request struct containing all necessary information to execute a command.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OperatorCommandRequest {
	command: String,
	target: Option<String>,
	state: Option<String>,
}

/// Route handler to dispatch a single manual operator command
pub async fn dispatch_operator_command(
	State(shared): State<Shared>,
	Json(request): Json<OperatorCommandRequest>,
) -> server::Result<()> {
	if let Some(flight) = shared.flight.0.lock().await.as_mut() {
		let command = match request.command.as_str() {
			"click_valve" => {
				let target = request.target
					.clone()
					.ok_or(bad_request("must supply target name"))?;
	
				let script = match request.state.as_deref() {
					Some("open") => format!("{target}.open()"),
					Some("closed") => format!("{target}.close()"),
					None => Err(bad_request("valve state is required"))?,
					_ => Err(bad_request("unrecognized state identifier"))?,
				};
	
				common::comm::FlightControlMessage::Sequence(Sequence { name: "command".to_owned(), script })
			},
			_ => return Err(bad_request("unrecognized command identifier")),
		};
	
		let serialized = postcard::to_allocvec(&command)
			.map_err(internal)?;
	
		flight
			.send_bytes(&serialized)
			.await
			.map_err(internal)?;
	} else {
		return Err(internal("flight computer not connected"));
	}

	Ok(())
}
