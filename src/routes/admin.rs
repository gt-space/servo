use actix_web::{error, web::{Data, Json}, Result};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use crate::{Database, extractors::User};
use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize)]
pub struct CreateUserRequest {
	pub username: String,
	pub is_admin: bool,
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateUserResponse;

/// A route function which creates a user without a password
pub async fn create_user(database: Data<Database>, request: Json<CreateUserRequest>, user: User) -> Result<Json<CreateUserResponse>> {
	if !user.is_admin {
		return Err(error::ErrorUnauthorized("admin access required"));
	}

	let salt = SaltString::generate(&mut OsRng).to_string();

	database.lock().await
		.execute(
			"INSERT INTO Users VALUES (?1, NULL, ?2, ?3)",
			rusqlite::params![&request.username, salt, request.is_admin as i32]
		)
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;
 
	Ok(Json(CreateUserResponse))
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExecuteSqlRequest {
	pub raw_sql: String
}

#[allow(missing_docs)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExecuteSqlResponse {
	pub column_names: Vec<String>,
	pub rows: Vec<Vec<serde_json::Value>>,
}

/// A route function which executes an arbitrary SQL query
pub async fn execute_sql(database: Data<Database>, request: Json<ExecuteSqlRequest>, user: User) -> Result<Json<ExecuteSqlResponse>> {
	if !user.is_admin {
		return Err(error::ErrorUnauthorized("admin access required"));
	}

	let database = database.lock().await;

	let mut sql = database
		.prepare(&request.raw_sql)
		.map_err(|error| error::ErrorBadRequest(error.to_string()))?;

	let column_names: Vec<String> = sql
		.column_names()
		.iter()
		.map(|name| name.to_string())
		.collect();

	let rows = sql.query_map([], |row| {
		Ok((0..column_names.len())
			.map(|c| {
				match row.get_ref_unwrap(c) {
					ValueRef::Null => serde_json::Value::Null,
					ValueRef::Integer(value) => serde_json::Value::Number(serde_json::Number::from(value)),
					ValueRef::Real(value) => serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()),
					ValueRef::Text(value) => serde_json::Value::String(String::from_utf8_lossy(value).to_string()),
					ValueRef::Blob(value) => {
						let byte_vec = value
							.iter()
							.map(|&n| serde_json::Value::Number(serde_json::Number::from(n)))
							.collect();

						serde_json::Value::Array(byte_vec)
					}
				}
			}).collect::<Vec<serde_json::Value>>())
	}).map_err(|error| error::ErrorBadRequest(error.to_string()))?
		.collect::<std::result::Result<Vec<_>, _>>()
		.unwrap();

	Ok(Json(ExecuteSqlResponse { column_names, rows: rows }))
}
