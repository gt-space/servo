use std::{fs, path::PathBuf};
use clap::ArgMatches;
use jeflog::fail;
use serde_json::json;

/// Tool function used to upload a sequence to be stored on the control server.
pub fn upload(args: &ArgMatches) {
	let sequence_path = args.get_one::<PathBuf>("sequence_path").unwrap();

	let name = sequence_path
		.file_stem()
		.expect("given path does not have a file stem")
		.to_string_lossy()
		.into_owned();

	let script = match fs::read(sequence_path) {
		Ok(raw) => base64::encode(raw),
		Err(error) => {
			fail!("Failed to read script from path: {error}");
			return;
		},
	};

	let client = reqwest::blocking::Client::new();
	let response = client.put("http://localhost:7200/operator/sequence")
		.json(&json!({
			"name": name,
			"script": script
		}))
		.send();

	if let Err(error) = response {
		fail!("Failed to send sequence update request: {error}");
	}
}
