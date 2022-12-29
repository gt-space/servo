use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Clone, Serialize, Deserialize)]
pub struct SqlResponse {
	pub column_names: Vec<String>,
	pub rows: Vec<Vec<serde_json::Value>>,
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

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthResponse {
	pub is_admin: bool,
	pub session_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LogsResponse {
	pub logs: Vec<TestLog>
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestResponse;
