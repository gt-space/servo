use serde_json::json;

/// Tool function used to send a sequence to be run on the flight computer.
pub async fn run(sequence: &str) -> anyhow::Result<()> {
	let client = reqwest::Client::new();
	let response = client.post("http://localhost:7200/operator/run-sequence")
		.json(&json!({
			"name": sequence,
			"force": true
		}))
		.send()
		.await?;

	println!("{response:#?}");

	Ok(())
}
