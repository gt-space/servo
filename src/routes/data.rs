use actix_web::{dev::PeerAddr, error, HttpResponse, Result, web::{Data, Json}};
use crate::{Database, extractors::User};
use serde::{Deserialize, Serialize};
use std::{time::{self, SystemTime, Duration}, net::SocketAddr, ops::Add};
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
	_user: User
) -> Result<Json<StartForwardingResponse>> {
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

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenewForwardingRequest {
	pub target_id: String,
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenewForardingResponse;

/// A route function which renews a forwarding session with the given target ID.
/// 
/// Must be requested from the machine with the same IP address as the
/// forwarding target being renewed. This route will not initiate a session.
pub async fn renew_forwarding(
	database: Data<Database>,
	request: Json<RenewForwardingRequest>,
	peer_address: PeerAddr,
	_user: User
) -> Result<HttpResponse> {
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
