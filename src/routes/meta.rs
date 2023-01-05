use actix_web::{error, Result, web::{Data, Json, Query}};
use crate::{Database, extractors::User};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Deserialize)]
pub struct LogsRequest {
	pub count: i32
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TestStatus {
	InProgress,
	Fail,
	Pass,
}

impl TestStatus {
	pub fn from_i32(num: i32) -> Option<Self> {
		match num {
			-1 => Some(Self::InProgress),
			0 => Some(Self::Fail),
			1 => Some(Self::Pass),
			_ => None,
		}
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestLog {
	pub log_id: String,
	pub test_id: String,
	pub initiator: String,
	pub start_time: i32,
	pub end_time: Option<i32>,
	pub status: TestStatus,
	pub message: Option<String>,	
}

impl Display for TestLog {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let color_code = match self.status {
			TestStatus::InProgress => "\x1b[34m",
			TestStatus::Fail => "\x1b[31m",
			TestStatus::Pass => "\x1b[32m",
		};

		let status = match self.status {
			TestStatus::InProgress => "....",
			TestStatus::Fail => "FAIL",
			TestStatus::Pass => "PASS",
		};

		let end_time = self.end_time.unwrap_or(-1);
		let message = self.message
			.as_deref()
			.unwrap_or("working on it...");

		write!(f, "{}[{}] ({} - {}) {} : {}", color_code, status, self.start_time, end_time, self.initiator, message)
	}
}

#[derive(Serialize)]
pub struct LogsResponse {
	pub logs: Vec<TestLog>
}

pub async fn get_logs(request: Query<LogsRequest>, database: Data<Database>, _user: User) -> Result<Json<LogsResponse>> {
	let database = database.lock().await;

	let mut sql = database
		.prepare("SELECT log_id, test_id, initiator, start_time, end_time, did_pass, message FROM TestLogs LIMIT ?1")
		.map_err(|_| error::ErrorInternalServerError("sql error"))?;

	let logs = sql.query_map(rusqlite::params![request.count], |row| {
		Ok(TestLog {
			log_id: row.get(0)?,
			test_id: row.get(1)?,
			initiator: row.get(2)?,
			start_time: row.get(3)?,
			end_time: row.get(4)?,
			status: TestStatus::from_i32(row.get::<_, Option<i32>>(5)?.unwrap_or(-1))
				.expect("invalid test status stored in database"),
			message: row.get(6)?,
		})
	}).map_err(|_| error::ErrorInternalServerError("sql error"))?
		.collect::<Result<Vec<TestLog>, _>>()
		.map_err(|_| error::ErrorInternalServerError("impossible execution path"))?;

	Ok(Json(LogsResponse { logs: logs }))
}
