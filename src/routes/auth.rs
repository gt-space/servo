use actix_web::{error, web::{Data, Json}, Result};
use argon2::{Argon2, PasswordHasher};
use tokio::time::MissedTickBehavior;
use crate::Database;
use serde::{Deserialize, Serialize};
use std::{time::{SystemTime, Duration, self}, sync::Arc, future::Future};
use uuid::Uuid;

#[allow(missing_docs)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthRequest {
	pub username: String,
	pub password: String,
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthResponse {
	pub is_admin: bool,
	pub session_id: String,
}

/// Authenticates a user given their username and password by assigning them a session and returning a session ID.
/// 
/// The session will be valid for 15 minutes unless a new request is sent 5 minutes before expiration. In other words, if no activity occurs for 15 minutes, then the session will deactivate.
/// However, if some activity occurs (sending a request to any authenticated endpoint) then the session will automatically update.

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

/// Periodically prunes sessions if they are more than 15 minutes old 
/// and makes them inactive forcing the user to re-authenticate
/// 
/// This function takes in a `&Arc<Mutex<SqlConnection>>` which is then downgraded to a weak
/// reference to the database. The returned Future loops until the database is dropped, at which
/// point it stops execution. This function is not intended to be used with `.await`, as it will
/// cause the current context to freeze.
/// 
/// # Improper Usage
/// 
/// This function is not intended to be used with `.await`, as it will cause the current context
/// to freeze until the database is dropped. Additionally, if a reference to the database is held
/// in the same context as the returned Future is executing, the program will not halt, as the database
/// will never be dropped since its strong reference count will never reach zero.
/// 
/// # Example
/// 
/// ```
/// // Prunes dead targets in the background every 10 seconds
/// tokio::spawn(routes::auth::prune_sessions(&database, Duration::from_secs(300)));
/// ```

pub fn prune_sessions(database: &Database, period: Duration) -> impl Future<Output = ()> {
	let weak_database = Arc::downgrade(database);

	async move {
		let mut prune_interval = tokio::time::interval(period);
		prune_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

		while let Some(database) = weak_database.upgrade() {
			let timestamp = SystemTime::now()
				.duration_since(time::UNIX_EPOCH)
				.expect("time is running backwards")
				.as_secs();

			database
				.lock()
				.await
				.execute("DELETE FROM Sessions WHERE ?1 - timestamp >= 900", rusqlite::params![timestamp])
				.unwrap();

			drop(database);
			prune_interval.tick().await;
		}
	}
}
