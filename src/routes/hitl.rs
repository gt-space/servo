use crate::{
	hitl,
	ThreadedDatabase,
	User,
	protocol::{
		TestRequest,
		TestResponse,
		TestDescription,
	},
};

use actix_web::{error, Result, web::{self, Json}};
use std::{env, fs};

pub async fn post_test(request: Json<TestRequest>, database: web::Data<ThreadedDatabase>, _user: User) -> Result<Json<TestResponse>> {
	if request.test_id.is_none() && request.test_description.is_none() {
		return Err(error::ErrorBadRequest("request must contain 'test_id' and/or 'test_description'"));
	}

	let mut test_id;

	if let Some(id) = &request.test_id {
		test_id = id.clone();
	} else {
		while {
			test_id = uuid::Uuid::new_v4()
				.to_string();

			database
				.lock()
				.unwrap()
				.query_row(
					"SELECT EXISTS(SELECT 1 FROM Tests WHERE id = ?1)",
					rusqlite::params![test_id],
					|row| Ok(row.get(0).unwrap())
				)
				.map_err(|_| error::ErrorInternalServerError("sql error"))?
		} {};
	}

	let test_description: TestDescription;

	if let Some(description) = &request.test_description {
		let home = env::var("HOME").map_err(|_| error::ErrorInternalServerError("home not set"))?;
		let test_path = format!("{home}/tests/{test_id}.json");

		let test_json = serde_json::to_string_pretty(description)
			.map_err(|_| error::ErrorInternalServerError("json parsing error"))?;

		fs::write(&test_path, test_json)
			.map_err(|_| error::ErrorInternalServerError("failed to write test to disk"))?;

		database
			.lock()
			.unwrap()
			.execute("INSERT OR REPLACE INTO Tests VALUES (?1, ?2, (SELECT runs FROM Tests WHERE id = ?1))", rusqlite::params![test_id, test_path])
			.map_err(|_| error::ErrorInternalServerError("sql error"))?;

		test_description = description.clone();
	} else {
		let test_path: String = database
			.lock()
			.unwrap()
			.query_row("SELECT file_path FROM Tests WHERE test_id = ?1", rusqlite::params![test_id], |row| row.get(0))
			.map_err(|_| error::ErrorInternalServerError("sql error"))?;
		
		let test_json = fs::read_to_string(test_path)
			.map_err(|_| error::ErrorInternalServerError("failed to read test file"))?;

		test_description = serde_json::from_str(&test_json)
			.map_err(|_| error::ErrorInternalServerError("failed to parse test file"))?;
	}

	let outcome = hitl::testing::run_test(&test_description);

	Ok(Json(TestResponse))
}
