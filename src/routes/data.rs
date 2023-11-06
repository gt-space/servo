use actix_web::{dev::PeerAddr, error, HttpResponse, Result, web::{Data, Json}};
use common::VehicleState;
use crate::Database;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::{self, SystemTime, Duration}, net::SocketAddr, ops::Add};
use uuid::Uuid;

const TARGET_LIFESPAN: Duration = Duration::from_secs(600);

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StartForwardingRequest {
	pub port: u16
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StartForwardingResponse {
	pub target_id: String,
	pub seconds_to_expiration: u64,
}

/// A route function which starts a forwarding session with the sender of the
/// request at the port specified.
/// 
/// Must be called from the machine which will be forwarded to. Forwarding
/// targets die after a default of 1 minute, so the API route corresponding to
/// `data::renew_forwarding` must be called before session death to maintain
/// the target.
pub async fn start_forwarding(
	database: Data<Database>,
	request: Json<StartForwardingRequest>,
	peer_address: PeerAddr,
) -> Result<Json<StartForwardingResponse>> {
	let database = database.connection().lock().await;

	let target_id = Uuid::new_v4().to_string();

	let mut target_address = peer_address.0;
	target_address.set_port(request.port);

	let expiration = SystemTime::now()
		.duration_since(time::UNIX_EPOCH)
		.map_err(|_| error::ErrorInternalServerError("time is running backwards"))?
		.add(TARGET_LIFESPAN)
		.as_secs();

	database
		.execute(
			"INSERT INTO ForwardingTargets VALUES (?1, ?2, ?3)",
			rusqlite::params![target_id, target_address.to_string(), expiration]
		)
		.map_err(|error| {
			match error {
				rusqlite::Error::SqliteFailure(rusqlite::ffi::Error { code: rusqlite::ffi::ErrorCode::ConstraintViolation, .. }, _) => error::ErrorConflict("target already exists"),
				_ => error::ErrorInternalServerError("sql error")
			}
		})?;

	Ok(Json(StartForwardingResponse {
		target_id,
		seconds_to_expiration: TARGET_LIFESPAN.as_secs()
	}))
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenewForwardingRequest {
	pub target_id: String,
}

/// A route function which renews a forwarding session with the given target ID.
/// 
/// Must be requested from the machine with the same IP address as the
/// forwarding target being renewed. This route will not initiate a session.
pub async fn renew_forwarding(
	database: Data<Database>,
	request: Json<RenewForwardingRequest>,
	peer_address: PeerAddr,
) -> Result<HttpResponse> {
	let database = database.connection().lock().await;

	let check_ip = database
		.query_row(
			"SELECT socket_address FROM ForwardingTargets WHERE target_id = ?1",
			rusqlite::params![request.target_id],
			|row| Ok(row.get::<_, String>(0)?),
		)
		.map_err(|error| {
			match error {
				rusqlite::Error::QueryReturnedNoRows => error::ErrorNotFound("target not found"),
				_ => error::ErrorInternalServerError("sql error"),
			}
		})?
		.parse::<SocketAddr>()
		.map_err(|_| error::ErrorInternalServerError("database parsing error"))?
		.ip();

	if check_ip != peer_address.0.ip() {
		return Err(error::ErrorForbidden("renewal request must originate from target ip"));
	}

	let expiration = SystemTime::now()
		.duration_since(time::UNIX_EPOCH)
		.map_err(|_| error::ErrorInternalServerError("time is running backwards"))?
		.add(TARGET_LIFESPAN)
		.as_secs();

	database
		.execute(
			"UPDATE ForwardingTargets SET expiration = ?1 WHERE target_id = ?2",
			rusqlite::params![expiration, request.target_id]
		)
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	Ok(HttpResponse::Ok().finish())
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
