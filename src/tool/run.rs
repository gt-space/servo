use serde_json::json;
use std::{fs, path::PathBuf};

/// Tool function used to send a sequence to be run on the flight computer.
pub async fn run(sequence_path: &str) -> anyhow::Result<()> {
	let sequence_path = PathBuf::from(sequence_path);

	let name = sequence_path
		.file_name()
		.unwrap()
		.to_string_lossy()
		.into_owned();

	let script = base64::encode(fs::read(sequence_path)?);

	let client = reqwest::Client::new();
	let response = client.post("http://localhost:7200/operator/sequence")
		.json(&json!({
			"name": name,
			"script": script
		}))
		.send()
		.await?;

	println!("{response:#?}");

	Ok(())
}
