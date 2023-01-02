use crate::{
	extractors::User,
	protocol::{CreateUserRequest, SqlRequest, SqlResponse},
	ThreadedDatabase,
};

use actix_web::{error, web::{Data, Json}, Result, HttpResponse};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use rusqlite::types::ValueRef;

pub async fn post_create_user(database: Data<ThreadedDatabase>, request: Json<CreateUserRequest>, user: User) -> Result<HttpResponse> {
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
 
	Ok(HttpResponse::Ok().finish())
}

pub async fn post_sql(database: Data<ThreadedDatabase>, request: Json<SqlRequest>, user: User) -> Result<Json<SqlResponse>> {
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

	Ok(Json(SqlResponse { column_names, rows: rows }))
}
