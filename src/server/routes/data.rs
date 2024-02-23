use axum::{extract::{ws, ConnectInfo, State, WebSocketUpgrade}, http::header, response::{IntoResponse, Response}, Json};
use common::comm::VehicleState;
use futures_util::{SinkExt, StreamExt};
use crate::server::{self, error::{bad_request, internal}, SharedState};
use jeflog::warn;
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;
use std::{collections::HashSet, net::SocketAddr, time::Duration};

/// Request struct for export requests.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportRequest {
	format: String,
	from: f64,
	to: f64,
}

/// Route function which exports all vehicle data from the database into a specified format.
pub async fn export(
	State(shared): State<SharedState>,
	Json(request): Json<ExportRequest>,
) -> server::Result<impl IntoResponse> {
	let database = shared.database
		.connection
		.lock()
		.await;

	let vehicle_states = database
		.prepare("SELECT recorded_at, vehicle_state FROM VehicleSnapshots WHERE recorded_at >= ?1 AND recorded_at <= ?2")
		.map_err(internal)?
		.query_map([request.from, request.to], |row| {
			let vehicle_state = postcard::from_bytes::<VehicleState>(&row.get::<_, Vec<u8>>(1)?)
				.map_err(|error| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Blob, Box::new(error)))?;

			Ok((row.get::<_, f64>(0)?, vehicle_state))
		})
		.and_then(|iter| iter.collect::<Result<Vec<_>, rusqlite::Error>>())
		.map_err(internal)?;

	match request.format.as_str() {
		"csv" => {
			let mut sensor_names = HashSet::new();
			let mut valve_names = HashSet::new();

			for (_, state) in &vehicle_states {
				for name in state.sensor_readings.keys() {
					// yes, a HashSet will not allow duplicate items even with a plain
					// insert, but the .clone() incurs a notable performance penalty,
					// and if it was just .insert(name.clone()) here, then it would clone
					// name every time despite the fact that it will rarely actually
					// need to be inserted. the same applies for valve_states.
					if !sensor_names.contains(name) {
						sensor_names.insert(name.clone());
					}
				}

				for name in state.valve_states.keys() {
					if !valve_names.contains(name) {
						valve_names.insert(name.clone());
					}
				}
			}

			let sensor_names = sensor_names
				.into_iter()
				.collect::<Vec<_>>();

			let valve_names = valve_names
				.into_iter()
				.collect::<Vec<_>>();

			let header = sensor_names
				.iter()
				.chain(valve_names.iter())
				.fold("timestamp".to_owned(), |header, name| header + "," + name);

			let mut content = header + "\n";

			for (timestamp, state) in vehicle_states {
				// first column is the timestamp
				content += &timestamp.to_string();

				for name in &sensor_names {
					let reading = state.sensor_readings.get(name);
					content += ",";

					// currently, if there is no data here, the column is empty.
					// we may want to change this.
					if let Some(reading) = reading {
						content += &reading.to_string();
					}
				}

				for name in &valve_names {
					let valve_state = state.valve_states.get(name);
					content += ",";

					// see comment in sensor readings above.
					if let Some(valve_state) = valve_state {
						content += &valve_state.to_string();
					}
				}

				content += "\n";
			}

			database.execute(
				"INSERT INTO VehicleSnapshots (from_time, to_time, format, contents) VALUES (?1, ?2, ?3, ?4)",
				rusqlite::params![request.from, request.to, request.format, content]
			).map_err(internal)?;

			let headers = [(header::CONTENT_TYPE, "text/csv; charset=utf-8")];
			Ok((headers, content))
		},
		_ => Err(bad_request("invalid export format")),
	}
}

/// Route function which accepts a WebSocket connection and begins forwarding vehicle state data.
pub async fn forward_data(
	ws: WebSocketUpgrade,
	State(shared): State<SharedState>,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
	ws.on_upgrade(move |socket| async move {
		let vehicle = shared.vehicle.clone();
		let (mut writer, mut reader) = socket.split();

		// spawn separate task for forwarding while the "main" task waits
		// until it can abort this task when the user wants to close
		let forwarding_handle = tokio::spawn(async move {
			let (vehicle_state, _) = vehicle.as_ref();

			// setup forwarding agent to send vehicle state every 100ms (10Hz)
			let mut interval = tokio::time::interval(Duration::from_millis(100));
			interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

			loop {
				let vehicle_state = vehicle_state
					.read()
					.await;

				// serialize vehicle state into JSON so it is easily digestible by the GUI.
				// vehicle state comes in as postcard and gets reserialized here. overhead isn't bad.
				let json = match serde_json::to_string(&*vehicle_state) {
					Ok(json) => json,
					Err(error) => {
						warn!("Failed to serialize vehicle state into JSON: {error}");
						continue;
					},
				};

				// drop vehicle state before sending to prevent unecessarily holding lock
				drop(vehicle_state);

				// attempt to forward vehicle state and break if connection is severed.
				if let Err(_error) = writer.send(ws::Message::Text(json)).await {
					warn!("Forwarding connection with peer \x1b[1m{}\x1b[0m severed.", peer);
					_ = writer.close().await;
					break;
				}

				// wait for 100ms to retransmit vehicle state
				interval.tick().await;
			}
		});

		// wait until reader from socket receives a ws::Message::Close or a None,
		// indicating that the stream is no longer readable
		while !matches!(reader.next().await, Some(Ok(ws::Message::Close(_))) | None) {}

		// cancel the forwarding stream upon receipt of a close message
		forwarding_handle.abort();
	})
}
