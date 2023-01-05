use actix_web::{error, web::{Data, Json}, Result};
use argon2::{Argon2, PasswordHasher};
use crate::{Database, protocol::{AuthRequest, AuthResponse}};
use std::time::SystemTime;
use uuid::Uuid;

pub async fn authenticate_user(request: Json<AuthRequest>, database: Data<Database>) -> Result<Json<AuthResponse>> {
	let database = database.lock().await;

	let (pass_hash, salt, is_admin): (Option<String>, String, bool) = database
		.query_row(
			"SELECT pass_hash, pass_salt, is_admin FROM Users WHERE username = ?1",
			rusqlite::params![request.username],
			|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
		)
		.map_err(|_| error::ErrorUnauthorized("user not found"))?;

	let argon = Argon2::default();
	let request_pass_hash = argon.hash_password(request.password.as_bytes(), &salt)
		.ok()
		.and_then(|t| t.hash)
		.ok_or(error::ErrorInternalServerError("failed to hash password"))?
		.to_string();

	if let Some(pass_hash) = pass_hash {
		if request_pass_hash != pass_hash {
			return Err(error::ErrorUnauthorized("password incorrect"));
		}
	} else {
		database.execute(
			"UPDATE Users SET pass_hash = ?1 WHERE username = ?2",
			rusqlite::params![request_pass_hash, request.username]
		).map_err(|_| error::ErrorInternalServerError("sql error"))?;
	}

	let session_id = Uuid::new_v4().to_string();
	let timestamp = SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_err(|_| error::ErrorInternalServerError("time is running backwards"))?
		.as_secs();

	database.execute("INSERT INTO Sessions VALUES (?1, ?2, ?3)", rusqlite::params![session_id, request.username, timestamp])
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	Ok(Json(AuthResponse { session_id, is_admin }))
}
