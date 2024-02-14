use actix_web::{error, web::{Data, Json}, HttpResponse};
use common::comm::NodeMapping;
use rusqlite::params;
use crate::{error::internal, flight::FlightComputer, Database};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request struct for getting mappings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GetMappingResponse {
	/// Array of all mappings in no specific order
	pub mappings: Vec<NodeMapping>
}

/// A route function which retrieves the current stored mappings.
pub async fn get_mappings(database: Data<Database>) -> actix_web::Result<Json<serde_json::Value>> {
	let database = database.connection().lock().await;

	let mappings = database
		.prepare("
			SELECT
				configuration_id,
				text_id,
				board_id,
				channel_type,
				channel,
				computer,
				max,
				min,
				calibrated_offset,
				connected_threshold,
				powered_threshold,
				normally_closed
			FROM NodeMappings
		")
		.map_err(|_| error::ErrorInternalServerError("error preparing sql statement"))?
		.query_and_then([], |row| {
			let configuration_id = row.get(0)?;

			let mapping = NodeMapping {
				text_id: row.get(1)?,
				board_id: row.get(2)?,
				channel_type: row.get(3)?,
				channel: row.get(4)?,
				computer: row.get(5)?,
				max: row.get(6)?,
				min: row.get(7)?,
				calibrated_offset: row.get(8)?,
				connected_threshold: row.get(9)?,
				powered_threshold: row.get(10)?,
				normally_closed: row.get(11)?,
			};

			Ok((configuration_id, mapping))
		})
		.map_err(|_| error::ErrorInternalServerError("failed to query database"))?
		.collect::<Result<Vec<(String, NodeMapping)>, rusqlite::Error>>()
		.map_err(|_| error::ErrorInternalServerError("failed to parse database entries into configuration"))?;

	let mut configurations = HashMap::<String, Vec<NodeMapping>>::new();

	for (configuration_id, mapping) in mappings {
		if let Some(config) = configurations.get_mut(&configuration_id) {
			config.push(mapping);
		} else {
			configurations.insert(configuration_id, vec![mapping]);
		}
	}

	Ok(Json(serde_json::to_value(&configurations)?))
}

/// Request struct for setting a mapping.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetMappingsRequest {
	/// An ID uniquely identifying the configuration being set or modified
	pub configuration_id: String,

	/// Array of all mappings in no specific order
	pub mappings: Vec<NodeMapping>
}

/// A route function which deletes and replaces a previous configuration
pub async fn post_mappings(
	database: Data<Database>,
	flight_computer: Data<FlightComputer>,
	request: Json<SetMappingsRequest>,
) -> actix_web::Result<HttpResponse> {
	let database = database.connection().lock().await;

	database
		.execute("DELETE FROM NodeMappings WHERE configuration_id = ?1", [&request.configuration_id])
		.map_err(internal)?;

	for mapping in &request.mappings {
		database
			.execute("
				INSERT INTO NodeMappings (
					configuration_id,
					text_id,
					board_id,
					channel_type,
					channel,
					computer,
					max,
					min,
					calibrated_offset,
					connected_threshold,
					powered_threshold,
					normally_closed,
					active
				) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, TRUE)
			", params![
				request.configuration_id,
				mapping.text_id,
				mapping.board_id,
				mapping.channel_type,
				mapping.channel,
				mapping.computer,
				mapping.max,
				mapping.min,
				mapping.calibrated_offset,
				mapping.connected_threshold,
				mapping.powered_threshold,
				mapping.normally_closed,
			])
			.map_err(|err| error::ErrorInternalServerError(format!("sql error: {}", err.to_string())))?;
	}

	drop(database);

	flight_computer
		.send_mappings()
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send mappings to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}

/// A route function which inserts a new mapping or updates an existing one
pub async fn put_mappings(
	database: Data<Database>,
	flight_computer: Data<FlightComputer>,
	request: Json<SetMappingsRequest>,
) -> actix_web::Result<HttpResponse> {
	let database = database.connection().lock().await;

	for mapping in &request.mappings {
		database.execute("
			INSERT INTO NodeMappings (
				configuration_id,
				text_id,
				board_id,
				channel_type,
				channel,
				computer,
				max,
				min,
				calibrated_offset,
				connected_threshold,
				powered_threshold,
				normally_closed,
				active
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, TRUE)
			ON CONFLICT (configuration_id, text_id) DO UPDATE SET
				board_id = excluded.board_id,
				channel = excluded.channel,
				channel_type = excluded.channel_type,
				computer = excluded.computer,
				scale = excluded.scale,
				offset = excluded.offset,
				connected_threshold = excluded.connected_threshold,
				powered_threshold = excluded.powered_threshold,
				normally_closed = excluded.normally_closed,
				active = excluded.active
		", params![
			request.configuration_id,
			mapping.text_id,
			mapping.board_id,
			mapping.channel_type,
			mapping.channel,
			mapping.computer,
			mapping.max,
			mapping.min,
			mapping.calibrated_offset,
			mapping.connected_threshold,
			mapping.powered_threshold,
			mapping.normally_closed,
		]).map_err(|_| error::ErrorInternalServerError("sql error"))?;
	}

	drop(database);

	flight_computer
		.send_mappings()
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send mappings to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}

/// The request struct used with the route function to delete mappings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteMappingsRequest {
	/// The configuration ID of the mappings being deleted.
	pub configuration_id: String,

	/// The mappings to be deleted. If this is `None`, then all mappings
	/// with the corresponding configuration ID will be deleted.
	pub mappings: Option<Vec<NodeMapping>>,
}

/// A route function which deletes the specified mappings.
pub async fn delete_mappings(
	database: Data<Database>,
	flight_computer: Data<FlightComputer>,
	request: Json<DeleteMappingsRequest>,
) -> actix_web::Result<HttpResponse> {
	let database = database.connection().lock().await;

	// if the mappings are specified, then only delete them
	// if not, then delete all mappings for that configuration (thus deleting the config)
	if let Some(mappings) = &request.mappings {
		for mapping in mappings {
			database
				.execute(
					"DELETE FROM NodeMappings WHERE configuration_id = ?1 AND text_id = ?2",
					params![request.configuration_id, mapping.text_id]
				)
				.map_err(|error| error::ErrorInternalServerError(error.to_string()))?;
		}
	} else {
		database
			.execute("DELETE FROM NodeMappings WHERE configuration_id = ?1", params![request.configuration_id])
			.map_err(|error| error::ErrorInternalServerError(error.to_string()))?;
	}

	drop(database);

	flight_computer
		.send_mappings()
		.await
		.map_err(|_| error::ErrorInternalServerError("failed to send mappings to flight computer"))?;

	Ok(HttpResponse::Ok().finish())
}

/// Request/response struct for getting and setting the active configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActiveConfiguration {
	configuration_id: String
}

/// A route function which activates a particular configuration
pub async fn activate_configuration(
	database: Data<Database>,
	request: Json<ActiveConfiguration>,
	flight_computer: Data<FlightComputer>,
) -> actix_web::Result<HttpResponse> {
	let database = database.connection().lock().await;

	database
		.execute("UPDATE NodeMappings SET active = FALSE WHERE active = TRUE", [])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	let rows_updated = database
		.execute("UPDATE NodeMappings SET active = TRUE WHERE configuration_id = ?1", [&request.configuration_id])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	drop(database);

	if rows_updated > 0 {
		flight_computer
			.send_mappings()
			.await
			.map_err(|_| error::ErrorInternalServerError("failed to send mappings to flight computer"))?;

		Ok(HttpResponse::Ok().finish())
	} else {
		Err(error::ErrorBadRequest("configuration_id does not exist"))
	}
}

/// A route function which returns the active configuration
pub async fn get_active_configuration(database: Data<Database>) -> actix_web::Result<Json<ActiveConfiguration>> {
	let configuration_id = database
		.connection()
		.lock()
		.await
		.query_row("SELECT configuration_id FROM NodeMappings WHERE active = TRUE", [], |row| row.get::<_, String>(0))
		.map_err(|_| error::ErrorNotFound("no configurations active"))?;

	Ok(Json(ActiveConfiguration { configuration_id }))
}

/// Route handler to calibrate all sensors in the current configuration.
pub async fn calibrate(
	database: Data<Database>,
	flight_computer: Data<FlightComputer>
) -> actix_web::Result<Json<HashMap<String, f64>>> {
	let vehicle_state = &flight_computer.vehicle_state().0;
	let database = database.connection().lock().await;

	let to_calibrate = database
		.prepare("
			SELECT text_id
			FROM NodeMappings
			WHERE
				channel_type IN ('current_loop', 'differential_signal')
				AND active
		")
		.map_err(internal)?
		.query_and_then([], |row| row.get(0))
		.map_err(internal)?
		.collect::<rusqlite::Result<Vec<String>>>()
		.map_err(internal)?;

	let vehicle_state = vehicle_state.lock().await;
	let mut updated = HashMap::new();

	for sensor in to_calibrate {
		if let Some(measurement) = vehicle_state.sensor_readings.get(&sensor) {
			database
				.execute("
					UPDATE NodeMappings
					SET calibrated_offset = ?1
					WHERE text_id = ?2
				", params![sensor, measurement.value])
				.map_err(internal)?;

			updated.insert(sensor.clone(), measurement.value);
		}
	}

	drop(database);
	drop(vehicle_state);

	flight_computer
		.send_mappings()
		.await
		.map_err(internal)?;

	Ok(Json(updated))
}
