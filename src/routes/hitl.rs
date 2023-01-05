use actix_web::{error, Result, web::{Data, Json}};
use crate::{control::Procedure, Database, extractors::User};
use serde::{Deserialize, Serialize};
use std::{env, fs};

#[derive(Deserialize)]
pub struct TestRequest {
	pub test_id: Option<String>,
	pub procedure: Option<Procedure>,
}

#[derive(Serialize)]
pub struct TestResponse;

pub async fn test_procedure(request: Json<TestRequest>, database: Data<Database>, _user: User) -> Result<Json<TestResponse>> {
	if request.test_id.is_none() && request.procedure.is_none() {
		return Err(error::ErrorBadRequest("request must contain 'test_id' and/or 'test_description'"));
	}

	let mut test_id;

	if let Some(id) = &request.test_id {
		test_id = id.clone();
	} else {
		while {
			test_id = uuid::Uuid::new_v4()
				.to_string();

			database.lock().await
				.query_row(
					"SELECT EXISTS(SELECT 1 FROM Tests WHERE id = ?1)",
					rusqlite::params![test_id],
					|row| Ok(row.get(0).unwrap())
				)
				.map_err(|_| error::ErrorInternalServerError("sql error"))?
		} {};
	}

	let test_procedure: Procedure;

	if let Some(procedure) = &request.procedure {
		let home = env::var("HOME").map_err(|_| error::ErrorInternalServerError("home not set"))?;
		let test_path = format!("{home}/tests/{test_id}.json");

		let test_json = serde_json::to_string_pretty(procedure)
			.map_err(|_| error::ErrorInternalServerError("json parsing error"))?;

		fs::write(&test_path, test_json)
			.map_err(|_| error::ErrorInternalServerError("failed to write test to disk"))?;

		database.lock().await
			.execute("INSERT OR REPLACE INTO Tests VALUES (?1, ?2, (SELECT runs FROM Tests WHERE id = ?1))", rusqlite::params![test_id, test_path])
			.map_err(|_| error::ErrorInternalServerError("sql error"))?;

		test_procedure = procedure.clone();
	} else {
		let test_path: String = database.lock().await
			.query_row("SELECT file_path FROM Tests WHERE test_id = ?1", rusqlite::params![test_id], |row| row.get(0))
			.map_err(|_| error::ErrorInternalServerError("sql error"))?;
		
		let test_json = fs::read_to_string(test_path)
			.map_err(|_| error::ErrorInternalServerError("failed to read test file"))?;

		test_procedure = serde_json::from_str(&test_json)
			.map_err(|_| error::ErrorInternalServerError("failed to parse test file"))?;
	}

	// let outcome = ctrl::testing::run_test(&test_procedure);
	unimplemented!();

	Ok(Json(TestResponse))
}
