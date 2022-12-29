use serde::{Serialize, Deserialize};

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
