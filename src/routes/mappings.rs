use actix_web::{error, web::{Data, Json}, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::{Database, extractors::User};

/// The mapping of an individual node.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeMapping {
	text_id: String,
	board_id: u32,
	channel: String,
	node_id: u32,
}

/// Request struct for getting mappings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GetMappingResponse {
	/// Array of all mappings in no specific order
	pub mappings: Vec<NodeMapping>
}

/// A route function which retrieves the current stored mappings.
pub async fn get_mappings(database: Data<Database>) -> actix_web::Result<Json<GetMappingResponse>> {
	use fs_protobuf_rust::compiled::mcfs::device::Channel;

	let mappings = database.lock().await
		.prepare("SELECT (text_id, board_id, channel, node_id) FROM NodeMappings")
		.map_err(|_| error::ErrorInternalServerError("error preparing sql statement"))?
		.query_map([], |row| {
			Ok(NodeMapping {
				text_id: row.get(0)?,
				board_id: row.get(1)?,
				channel: match Channel::from(row.get::<_, i32>(2)?) {
					Channel::GPIO => "gpio",
					Channel::LED => "led",
					Channel::RAIL_3V3 => "rail_3v3",
					Channel::RAIL_5V => "rail_5v",
					Channel::RAIL_5V5 => "rail_5v5",
					Channel::RAIL_24V => "rail_24v",
					Channel::CURRENT_LOOP => "current_loop",
					Channel::DIFFERENTIAL_SIGNAL => "differential_signal",
					Channel::TEMPERATURE_DETECTOR => "temperature_detector",
					Channel::VALVE => "valve",
				}.to_string(),
				node_id: row.get(3)?,
			})
		})
		.map_err(|_| error::ErrorInternalServerError("failed to query database"))?
		.collect::<Result<Vec<_>, _>>()
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	Ok(Json(GetMappingResponse { mappings }))
}

/// Request struct for setting a mapping.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetMappingsRequest {
	/// Array of all mappings in no specific order
	pub mappings: Vec<NodeMapping>
}

/// A route function which inserts a new mapping or updates an existing one
pub async fn set_mappings(
	database: Data<Database>,
	request: Json<SetMappingsRequest>,
	_user: User,
) -> actix_web::Result<HttpResponse> {
	let database = database.lock().await;

	for mapping in &request.mappings {
		use fs_protobuf_rust::compiled::mcfs::device::Channel;

		let channel = match mapping.channel.to_lowercase().as_str() {
			"gpio" => Channel::GPIO,
			"led" => Channel::LED,
			"rail_3v3" => Channel::RAIL_3V3,
			"rail_5v" => Channel::RAIL_5V,
			"rail_5v5" => Channel::RAIL_5V5,
			"rail_24v" => Channel::RAIL_24V,
			"current_loop" => Channel::CURRENT_LOOP,
			"differential_signal" => Channel::DIFFERENTIAL_SIGNAL,
			"temperature_detector" => Channel::TEMPERATURE_DETECTOR,
			"valve" => Channel::VALVE,
			_ => Err(error::ErrorBadRequest("invalid channel name"))?
		} as i32;

		database.execute("
			INSERT INTO NodeMappings (text_id, board_id, channel, node_id) VALUES (?1, ?2, ?3, ?4)
			ON CONFLICT(text_id) DO UPDATE SET
				board_id = excluded.board_id,
				channel = excluded.channel,
				node_id = excluded.node_id
		", rusqlite::params![mapping.text_id, mapping.board_id, channel, mapping.node_id])
			.map_err(|_| error::ErrorInternalServerError("sql error"))?;
	}

	Ok(HttpResponse::Ok().finish())
}