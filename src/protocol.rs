use std::fmt::Display;

use serde::{Serialize, Deserialize};
use num_derive::FromPrimitive;

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthRequest {
	pub username: String,
	pub password: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
	pub username: String,
	pub is_admin: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LogsRequest {
	pub count: i32
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SqlRequest {
	pub raw_sql: String
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SqlResponse {
	pub column_names: Vec<String>,
	pub rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TestAction {
	OPEN,
	CLOSE,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TestTarget {
	OVENT,
	PRVENT,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestEvent {
	pub action: TestAction,
	pub target: TestTarget,
	pub t: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestStage {
	pub name: String,
	pub sequence: Vec<TestEvent>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestDescription {
	pub stages: Vec<TestStage>
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TestRequest {
	pub test_id: Option<String>,
	pub test_description: Option<TestDescription>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PackagedFile {
	pub relative_path: String,
	pub file_name: String,

	#[serde(with="base64")]
	pub contents: Vec<u8>,
}

mod base64 {
	use serde::{Serialize, Deserialize, Serializer, Deserializer};

	pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
		String::serialize(&base64::encode(v), s)
	}

	pub fn deserialize<'a, D: Deserializer<'a>>(d: D) -> Result<Vec<u8>, D::Error> {
		base64::decode(String::deserialize(d)?.as_bytes())
			.map_err(|error| serde::de::Error::custom(error))
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UploadRequest {
	pub files: Vec<PackagedFile>,
	pub session_id: String,
}

#[derive(Clone, Serialize, Deserialize, FromPrimitive)]
pub enum TestStatus {
	InProgress = -1,
	Fail = 0,
	Pass = 1,
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
