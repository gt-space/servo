use crate::{
	extractors::User,
	protocol::{
		LogsRequest,
		LogsResponse,
		TestLog,
		TestStatus,
	},
	ThreadedDatabase,
};

use actix_web::{error, Result, web::{self, Json, Query}};

pub async fn get_logs(request: Query<LogsRequest>, database: web::Data<ThreadedDatabase>, _user: User) -> Result<Json<LogsResponse>> {
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
