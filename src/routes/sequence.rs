use actix_web::{error, web::{Data, Json}, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::flight::FlightComputer;

/// Request struct for setting/sending sequences.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SequenceRequest {
	name: String,
	script: String,
}

/// Route function which receives a sequence and sends it directly to the flight computer.
pub async fn run_sequence(
	flight_computer: Data<FlightComputer>,
	request: Json<SequenceRequest>,
) -> actix_web::Result<HttpResponse> {
	let decoded_script = base64::decode(&request.script)
		.map_err(|_| error::ErrorBadRequest("base64 sequence script could not be decoded"))
		.and_then(|bytes|
			String::from_utf8(bytes)
				.map_err(|_| error::ErrorBadRequest("failed to parse raw bytes into valid string"))
		)?;

	flight_computer.send_sequence(&request.name, &decoded_script).await?;
	Ok(HttpResponse::Ok().finish())
}
