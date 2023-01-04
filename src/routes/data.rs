use actix_web::{dev::PeerAddr, error, HttpResponse, Result, web::{Data, Json}};
use crate::{extractors::User, ThreadedDatabase};
use serde::{Deserialize, Serialize};
use std::{time::{self, SystemTime, Duration}, net::SocketAddr, ops::Add};
use uuid::Uuid;

const TARGET_LIFESPAN: Duration = Duration::from_secs(600);

#[derive(Deserialize)]
pub struct StartForwardingRequest {
	port: u16
}

#[derive(Serialize)]
pub struct StartForwardingResponse {
	target_id: String,
	seconds_to_expiration: u64,
}

pub async fn start_forwarding(database: Data<ThreadedDatabase>, request: Json<StartForwardingRequest>, peer_address: PeerAddr, _user: User) -> Result<Json<StartForwardingResponse>> {
	let database = database.lock().await;

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

#[derive(Deserialize)]
pub struct RenewForwardingRequest {
	target_id: String,
}

pub async fn renew_forwarding(database: Data<ThreadedDatabase>, request: Json<RenewForwardingRequest>, peer_address: PeerAddr, _user: User) -> Result<HttpResponse> {
	let database = database.lock().await;

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
