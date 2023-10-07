use actix_web::{error, web::{Data, Json}, Result};
use crate::Database;
use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};

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
pub async fn execute_sql(database: Data<Database>, request: Json<ExecuteSqlRequest>) -> Result<Json<ExecuteSqlResponse>> {
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
