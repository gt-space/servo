use axum::{extract::State, Json};
use common::comm::Sequence;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::server::{self, error::{bad_request, internal}, SharedState};

/// Used in sequences response struct to attach the configuration ID.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SequenceWithConfiguration {
	/// The name of the sequence.
	pub name: String,

	/// The Python sequence script.
	pub script: String,

	/// The ID of the configuration associated with the sequence.
	pub configuration_id: Option<String>,
}

/// Response struct for getting the sequences stored in the database.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RetrieveSequenceResponse {
	/// The collection of all sequences present on the control server.
	pub sequences: Vec<SequenceWithConfiguration>
}

/// Route function to retrieve all sequences from the database.
pub async fn retrieve_sequences(State(shared): State<SharedState>) -> server::Result<Json<RetrieveSequenceResponse>> {
	let sequences = shared.database
		.connection
		.lock()
		.await
		.prepare("SELECT name, script, configuration_id FROM Sequences")
		.map_err(internal)?
		.query_map([], |row| {
			Ok(SequenceWithConfiguration {
				name: row.get(0)?,
				script: row.get(1)?,
				configuration_id: row.get(2)?,
			})
		})
		.map_err(internal)?
		.collect::<Result<Vec<_>, _>>()
		.map_err(internal)?;

	Ok(Json(RetrieveSequenceResponse { sequences }))
}

/// Request struct for saving a sequence without running it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SaveSequenceRequest {
	/// The unqiue name of the sequence that identifies it.
	pub name: String,

	/// The ID of the associated configuration (provides extra check).
	pub configuration_id: Option<String>,

	/// The Base64-encoded script to save.
	pub script: String,
}

/// A route function which saves a sequence without running it.
pub async fn save_sequence(
	State(shared): State<SharedState>,
	Json(request): Json<SaveSequenceRequest>,
) -> server::Result<()> {
	let decoded_script = base64::decode(&request.script)
		.map_err(bad_request)
		.and_then(|bytes| {
			String::from_utf8(bytes)
				.map_err(bad_request)
		})?;

	shared.database
		.connection
		.lock()
		.await
		.execute(
			"INSERT OR REPLACE INTO Sequences (name, configuration_id, script) VALUES (?1, ?2, ?3)",
			params![request.name, request.configuration_id, decoded_script]
		)
		.map_err(internal)?;

	Ok(())
}

/// Request struct to delete a sequence from the database.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteSequenceRequest {
	/// The name stored in the database identifying the sequence to be deleted.
	pub name: String
}

/// Route function to delete a sequence from the database.
pub async fn delete_sequence(
	State(shared): State<SharedState>,
	Json(request): Json<DeleteSequenceRequest>,
) -> server::Result<()> {
	shared.database
		.connection
		.lock()
		.await
		.execute("DELETE FROM Sequences WHERE text_id = ?1", [&request.name])
		.map_err(bad_request)?;

	Ok(())
}

/// Request struct for running a sequence on the flight computer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RunSequenceRequest {
	/// The name of the sequence to run, as recorded in the database.
	pub name: String,

	/// Force the sequence to be executed, even if the configuration IDs do not match.
	pub force: Option<bool>,
}

/// Route function which receives a sequence and sends it directly to the flight computer.
pub async fn run_sequence(
	State(shared): State<SharedState>,
	Json(request): Json<RunSequenceRequest>,
) -> server::Result<()> {
	// TODO: Add check for active configuration against the configuration_id in the database

	let sequence = shared.database
		.connection
		.lock()
		.await
		.query_row("SELECT script FROM Sequences WHERE name = ?1", [&request.name], |row| {
			Ok(Sequence {
				name: request.name.clone(),
				script: row.get(0)?,
			})
		})
		.map_err(bad_request)?;

	if let Some(flight) = shared.flight.0.lock().await.as_mut() {
		flight.send_sequence(sequence)
			.await
			.map_err(internal)?;
	} else {
		return Err(internal("flight computer not connected"));
	}

	Ok(())
}
