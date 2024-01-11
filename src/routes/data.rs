use actix_web::{error, HttpResponse, Result, web::{Data, Json}};
use common::VehicleState;
use crate::{Database, forwarding::ForwardingAgent};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

/// Starts a stream over HTTP that forwards vehicle state at regular intervals
pub async fn forward(forwarding_agent: Data<Arc<ForwardingAgent>>) -> HttpResponse {
	HttpResponse::Ok().streaming(forwarding_agent.stream())
}

/// Request struct for export requests.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportRequest {
	format: String,
	from: f64,
	to: f64,
}

/// Route function which exports all vehicle data from the database into a specified format.
pub async fn export(
	database: Data<Database>,
	request: Json<ExportRequest>,
) -> Result<HttpResponse> {
	let database = database.connection().lock().await;

	let vehicle_states = database
		.prepare("SELECT recorded_at, vehicle_state FROM VehicleSnapshots WHERE recorded_at >= ?1 AND recorded_at <= ?2")
		.map_err(|error| error::ErrorInternalServerError(error.to_string()))?
		.query_map([request.from, request.to], |row| {
			let vehicle_state = postcard::from_bytes::<VehicleState>(&row.get::<_, Vec<u8>>(1)?)
				.map_err(|error| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Blob, Box::new(error)))?;

			Ok((row.get::<_, f64>(0)?, vehicle_state))
		})
		.and_then(|iter| iter.collect::<Result<Vec<_>, rusqlite::Error>>())
		.map_err(|error| error::ErrorInternalServerError(error.to_string()))?;

	match request.format.as_str() {
		"csv" => {
			let mut sensor_names = HashSet::new();
			let mut valve_names = HashSet::new();

			for (_, state) in &vehicle_states {
				for name in state.sensor_readings.keys() {
					// yes, a HashSet will not allow duplicate items even with a plain
					// insert, but the .clone() incurs a notable performance penalty,
					// and if it was just .insert(name.clone()) here, then it would clone
					// name every time despite the fact that it will rarely actually
					// need to be inserted. the same applies for valve_states.
					if !sensor_names.contains(name) {
						sensor_names.insert(name.clone());
					}
				}

				for name in state.valve_states.keys() {
					if !valve_names.contains(name) {
						valve_names.insert(name.clone());
					}
				}
			}

			let sensor_names = sensor_names
				.into_iter()
				.collect::<Vec<_>>();

			let valve_names = valve_names
				.into_iter()
				.collect::<Vec<_>>();

			let header = sensor_names
				.iter()
				.chain(valve_names.iter())
				.fold("timestamp".to_owned(), |header, name| header + "," + name);

			let mut content = header + "\n";

			for (timestamp, state) in vehicle_states {
				// first column is the timestamp
				content += &timestamp.to_string();

				for name in &sensor_names {
					let reading = state.sensor_readings.get(name);
					content += ",";

					if let Some(reading) = reading {
						content += &reading.to_string();
					}
				}

				for name in &valve_names {
					let valve_state = state.valve_states.get(name);
					content += ",";

					if let Some(valve_state) = valve_state {
						content += &valve_state.to_string();
					}
				}

				content += "\n";
			}

			database.execute(
				"INSERT INTO VehicleSnapshots (from_time, to_time, format, contents) VALUES (?1, ?2, ?3, ?4)",
				rusqlite::params![request.from, request.to, request.format, content]
			).map_err(|error| error::ErrorInternalServerError(error.to_string()))?;

			Ok(
				HttpResponse::Ok()
					.content_type("text/csv")
					.body(content)
			)
		},
		_ => Err(error::ErrorBadRequest("invalid export format")),
	}
}
