use actix_web::{error, web::{Data, Json}, HttpResponse};
use crate::Database;
use serde::{Deserialize, Serialize};

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
	use fs_protobuf_rust::compiled::mcfs::board::ChannelType;

	let mappings = database.lock().await
		.prepare("SELECT (text_id, board_id, channel, node_id) FROM NodeMappings")
		.map_err(|_| error::ErrorInternalServerError("error preparing sql statement"))?
		.query_map([], |row| {
			Ok(NodeMapping {
				text_id: row.get(0)?,
				board_id: row.get(1)?,
				channel: match ChannelType::from(row.get::<_, i32>(2)?) {
					ChannelType::GPIO => "gpio",
					ChannelType::LED => "led",
					ChannelType::RAIL_3V3 => "rail_3v3",
					ChannelType::RAIL_5V => "rail_5v",
					ChannelType::RAIL_5V5 => "rail_5v5",
					ChannelType::RAIL_24V => "rail_24v",
					ChannelType::CURRENT_LOOP => "current_loop",
					ChannelType::DIFFERENTIAL_SIGNAL => "differential_signal",
					ChannelType::TC => "tc",
					ChannelType::RTD => "rtd",
					ChannelType::VALVE_CURRENT => "valve_current",
					ChannelType::VALVE_VOLTAGE => "valve_voltage",
					ChannelType::VALVE => "valve",
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
pub async fn set_mappings(database: Data<Database>, request: Json<SetMappingsRequest>) -> actix_web::Result<HttpResponse> {
	let database = database.lock().await;

	for mapping in &request.mappings {
		use fs_protobuf_rust::compiled::mcfs::board::ChannelType;

		let channel = match mapping.channel.to_lowercase().as_str() {
			"gpio" => ChannelType::GPIO,
			"led" => ChannelType::LED,
			"rail_3v3" => ChannelType::RAIL_3V3,
			"rail_5v" => ChannelType::RAIL_5V,
			"rail_5v5" => ChannelType::RAIL_5V5,
			"rail_24v" => ChannelType::RAIL_24V,
			"current_loop" => ChannelType::CURRENT_LOOP,
			"differential_signal" => ChannelType::DIFFERENTIAL_SIGNAL,
			"tc" => ChannelType::TC,
			"valve_current" => ChannelType::VALVE_CURRENT,
			"valve_voltage" => ChannelType::VALVE_VOLTAGE,
			"rtd" => ChannelType::RTD,
			"valve" => ChannelType::VALVE,
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
