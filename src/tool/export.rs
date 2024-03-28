use jeflog::fail;
use serde_json::json;
use std::{fs, path::PathBuf, time::Duration};

/// Function for requesting all data between two timestamps as stored on the ground server.
/// Used in the export command line routing.
pub fn export(from: Option<f64>, to: Option<f64>, output_path: &str) {
	let output_path = PathBuf::from(output_path);

	let from = from.unwrap_or(0.0);
	let to = to.unwrap_or(f64::MAX);

	let export_format = output_path
		.extension()
		.unwrap()
		.to_string_lossy();

	let client = reqwest::blocking::Client::new();
	let export_result = client.post("http://localhost:7200/data/export")
		.json(&json!({
			"format": export_format,
			"from": from,
			"to": to
		}))
		.timeout(Duration::from_secs(3600))
		.send()
		.and_then(|content| content.bytes());

	let bytes = match export_result {
		Ok(bytes) => bytes,
		Err(error) => {
			fail!("Failed to request and parse data export: {error}");
			return;
		},
	};

	if let Err(error) = fs::write(&output_path, bytes) {
		fail!("Failed to write to \x1b[1m{}\x1b[0m: {error}", output_path.to_string_lossy());
		return;
	}
}
