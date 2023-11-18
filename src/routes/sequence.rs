use actix_web::{web::{Data, Json}, HttpResponse};
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
	flight_computer.send_sequence(&request.script).await?;
	Ok(HttpResponse::Ok().finish())
}
