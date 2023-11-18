use actix_web::{error, web::{Data, Json}, HttpResponse};
use crate::{Database, flight::FlightComputer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The mapping of an individual node.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeMapping {
	/// The text identifier, or name, of the node.
	pub text_id: String,
	
	/// A number identifying which SAM board the node is on.
	pub board_id: u32,

	/// The channel type of the node, such as "valve".
	pub channel_type: String,

	/// A number identifying which channel on the SAM board controls the node.
	pub channel: u32,

	/// Which computer controls the SAM board, "flight" or "ground".
	pub computer: String,
}

/// Request struct for getting mappings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GetMappingResponse {
	/// Array of all mappings in no specific order
	pub mappings: Vec<NodeMapping>
}

/// A route function which retrieves the current stored mappings.
pub async fn get_mappings(database: Data<Database>) -> actix_web::Result<Json<serde_json::Value>> {
	let database = database.lock().await;

	let mappings = database
		.prepare("SELECT configuration_id, text_id, board_id, channel_type, channel, computer FROM NodeMappings")
		.map_err(|_| error::ErrorInternalServerError("error preparing sql statement"))?
		.query_and_then([], |row| {
			let configuration_id = row.get(0)?;

			let mapping = NodeMapping {
				text_id: row.get(1)?,
				board_id: row.get(2)?,
				channel_type: row.get(3)?,
				channel: row.get(4)?,
				computer: row.get(5)?,
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
	let database = database.lock().await;

	database
		.execute("DELETE FROM NodeMappings WHERE configuration_id = ?1", [&request.configuration_id])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	for mapping in &request.mappings {
		database
			.execute("
				INSERT INTO NodeMappings (configuration_id, text_id, board_id, channel_type, channel, computer)
				VALUES (?1, ?2, ?3, ?4, ?5, ?6)
			", rusqlite::params![
				request.configuration_id,
				mapping.text_id,
				mapping.board_id,
				mapping.channel_type,
				mapping.channel,
				mapping.computer,
			])
			.map_err(|err| error::ErrorInternalServerError(format!("sql error: {}", err.to_string())))?;
		}

	flight_computer.send_mappings(&request.mappings).await?;

	Ok(HttpResponse::Ok().finish())
}

/// A route function which inserts a new mapping or updates an existing one
pub async fn put_mappings(
	database: Data<Database>,
	flight_computer: Data<FlightComputer>,
	request: Json<SetMappingsRequest>,
) -> actix_web::Result<HttpResponse> {
	let database = database.lock().await;

	for mapping in &request.mappings {
		database.execute("
			INSERT INTO NodeMappings (configuration_id, text_id, board_id, channel_type, channel, computer)
			VALUES (?1, ?2, ?3, ?4, ?5, ?6)
			ON CONFLICT (configuration_id, text_id) DO UPDATE SET
				board_id = excluded.board_id,
				channel = excluded.channel,
				channel_type = excluded.channel_type,
				computer = excluded.computer
		", rusqlite::params![
			request.configuration_id,
			mapping.text_id,
			mapping.board_id,
			mapping.channel_type,
			mapping.channel,
			mapping.computer,
		]).map_err(|_| error::ErrorInternalServerError("sql error"))?;
	}

	flight_computer.send_mappings(&request.mappings).await?;

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
) -> actix_web::Result<HttpResponse> {
	let database = database.lock().await;

	database
		.execute("UPDATE NodeMappings SET active = FALSE WHERE active = TRUE", [])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	let rows_updated = database
		.execute("UPDATE NodeMappings SET active = TRUE WHERE configuration_id = ?1", [&request.configuration_id])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	if rows_updated > 0 {
		Ok(HttpResponse::Ok().finish())
	} else {
		Err(error::ErrorBadRequest("configuration_id does not exist"))
	}
}

/// A route function which returns the active configuration
pub async fn get_active_configuration(database: Data<Database>) -> actix_web::Result<Json<ActiveConfiguration>> {
	let configuration_id = database
		.lock()
		.await
		.query_row("SELECT configuration_id FROM NodeMappings WHERE active = TRUE", [], |row| row.get::<_, String>(0))
		.map_err(|_| error::ErrorNotFound("no configurations active"))?;

	Ok(Json(ActiveConfiguration { configuration_id }))
}
