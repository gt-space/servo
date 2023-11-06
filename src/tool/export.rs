use serde_json::json;
use tokio::fs;
use std::path::PathBuf;

pub async fn export(from: Option<f64>, to: Option<f64>, output_path: &str) -> anyhow::Result<()> {
	let output_path = PathBuf::from(output_path);

	let from = from.unwrap_or(0.0);
	let to = to.unwrap_or(f64::MAX);

	let export_format = output_path
		.extension()
		.unwrap()
		.to_string_lossy();

	let client = reqwest::Client::new();
	let export_content = client.post("http://localhost:7200/data/export")
		.json(&json!({
			"format": export_format,
			"from": from,
			"to": to
		}))
		.send()
		.await?
		.text()
		.await?;

	fs::write(output_path, export_content).await?;
	Ok(())
}
