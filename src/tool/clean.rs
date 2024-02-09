use jeflog::pass;
use std::path::Path;
use tokio::fs;

/// Simple tool function used to clean the servo directory and database.
pub async fn clean(servo_dir: &Path) -> anyhow::Result<()> {
	fs::remove_dir_all(servo_dir).await?;
	pass!("Cleaned Servo directory.");

	Ok(())
}
