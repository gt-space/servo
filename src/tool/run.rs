use jeflog::fail;
use serde_json::json;

/// Tool function used to send a sequence to be run on the flight computer.
pub fn run(sequence: &str) {
	let client = reqwest::blocking::Client::new();
	let response = client
		.post("http://localhost:7200/operator/run-sequence")
		.json(&json!({
			"name": sequence,
			"force": true
		}))
		.send();

	if let Err(error) = response {
		fail!("Failed to run sequence: {error}");
	}
}
