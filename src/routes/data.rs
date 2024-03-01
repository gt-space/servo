use actix_web::{error, HttpResponse, Result, web::{Data, Json}};
use common::comm::{ValveState, VehicleState, Measurement, Unit};
use crate::{Database, forwarding::ForwardingAgent};
use crate::error::internal;
use hdf5;
use hdf5::{Group, Object, Dataset, DatasetBuilder, DatasetBuilderData}; // Does not include File to avoid overlaps
use std::sync::atomic::{AtomicU32, Ordering};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use std::path::Path;


/// Starts a stream over HTTP that forwards vehicle state at regular intervals
pub async fn forward(forwarding_agent: Data<Arc<ForwardingAgent>>) -> HttpResponse {
	HttpResponse::Ok().streaming(forwarding_agent.stream())
}

/// Request struct for export requests.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportRequest {
	format: String,
	from: f64,
	to: f64,
}

// An integer used to create unique filenames for exports in case two exports overlap in time
// Atomic to be safe
static EXPORT_FILE_INDEX_ATOMIC : AtomicU32 = AtomicU32::new(0);

#[cfg_attr(not(debug_assertions), no_panic::no_panic)]
#[inline(never)]
/// A function that creates an HDF5 file at a given path containing the timestamps, sensor, and valve values in each vehicle state as specified in sensor_names and valve_names
pub fn make_hdf5_file(sensor_names : &[std::string::String], valve_names : &[std::string::String], vehicle_states : &[(f64, VehicleState)], path : &Path) -> hdf5::Result<()>{
	// Create the HDF5 file
	let file : hdf5::File = hdf5::File::create(path)?;
	
	// Create the organizational groups
	let reading_metadata_group : Group = file.create_group("metadata")?;
	let valve_state_ids_group : Group = reading_metadata_group.create_group("valve_state_ids")?;
	
	
	let sensors_group : Group = file.create_group("sensors")?;
	let valves_group : Group = file.create_group("valves")?;
	
	// Initialize with the size of the vehicle state vector, since we'll have equal count of them
	let mut timestamps_vec : Vec<f64> = Vec::<f64>::with_capacity(vehicle_states.len());
	
	// Turn timestamps into dataset
	for (timestamp, _) in vehicle_states {
		timestamps_vec.push(*timestamp);
	}
	let _  : Dataset = DatasetBuilder::new(&reading_metadata_group)
		.with_data(&timestamps_vec)
		.create("timestamps")?;
		
	for name in sensor_names {
		let mut reading_vec : Vec<f64> = Vec::<f64>::with_capacity(vehicle_states.len());
		let mut unit_vec : Vec<i8> = Vec::<i8>::with_capacity(vehicle_states.len());
		
		// Yes I know iterating through the vehicle states for every sensor / valve is dumb,
		// but I'm avoiding storing the entirety of the vehicle state in memory twice, so each
		// sensor is grabbed seperately
		for (_, state) in vehicle_states {
			let value = state.sensor_readings.get(name);
			// Put in bad data if nothing is found
			match value {
				Some(x) =>  { 
					reading_vec.push(x.value);
					let id : i8 = (x.unit as i8).try_into()?; // Should never panic unless absurd amounts of units are added
					unit_vec.push(id);
					},
				// Immature but nobody will see this and not realize it's garbage data.
				// Might replace with an infinity or something
				None => {
					reading_vec.push(-6942069420.0);
					unit_vec.push(-69);
				},
			};
		}
		let curr_sensor_group = sensors_group.create_group(name.as_str())?;
		
		// Make datasets
		let _ : Dataset = curr_sensor_group.new_dataset_builder()
			.deflate(9)
			.with_data(&reading_vec)
			.create("readings")?;
		
		let _ : Dataset = curr_sensor_group.new_dataset_builder()
			.deflate(9)
			.with_data(&unit_vec)
			.create("units")?;
	}
	
	
	// A vector of all the possible ValveStates seen. Used to create the attributes that indicate what each value of ValveState means.
	// Likely more efficient as a simple vector, since ValveState has few possible elements. Will check later.
	// I was originally going to make this a single attribute in the metadata category, but you can't iterate through an enum, 
	// so I'll talk to Jeff about making a possible ValveState iter to replace this.
	let mut seen_valve_states = HashSet::<ValveState>::new();
	
	// Will make all values of valves metadata later
	for name in valve_names {
		// A vector of all the values of the valve in each timeframe
		let mut state_vec : Vec<i32> = Vec::<i32>::with_capacity(vehicle_states.len());
		
		// Yes I know iterating through the vehicle states for every sensor / valve is dumb,
		// but I'm avoiding storing the entirety of the vehicle state in memory twice, so each
		// sensor is grabbed seperately
		for (_, state) in vehicle_states {
			let measurement = state.valve_states.get(name);
			// Put in bad data if nothing is found
			match measurement {
				Some(x) => {
					if !seen_valve_states.contains(x) { // Keep track of seen valve states
						seen_valve_states.insert(x.clone());
					}
					state_vec.push((*x as i8).try_into()?)
					},
				// Immature but nobody will see this and not realize it's garbage data.
				// Might replace with an infinity or something, will go over with Jeff.
				None => state_vec.push(-69),
			};
		}
		
		// Make dataset
		let _  : Dataset = valves_group.new_dataset_builder()
			.deflate(9)
			.with_data(&state_vec)
			.create(name.as_str())?;
	}
	
	// Put an attribute of what id each valve state is represented by into the valve state id's metadata group
	// TLDR; it's an enum of attributes on a folder
	for state in seen_valve_states {
		let attr = valve_state_ids_group.new_attr::<i8>().shape(1).create(state.to_string().as_str())?;
		let id : i8 = (state as i8).try_into()?;
		let _ = attr.write(&[id]);
	}
	
	// Close the file
	let _ = file.close()?;
	
	Ok(())
}

/// Route function which exports all vehicle data from the database into a specified format.
pub async fn export(
	database: Data<Database>,
	request: Json<ExportRequest>,
) -> Result<HttpResponse> {
	let database = database.connection().lock().await;

	let vehicle_states = database
		.prepare("SELECT recorded_at, vehicle_state FROM VehicleSnapshots WHERE recorded_at >= ?1 AND recorded_at <= ?2")
		.map_err(|error| error::ErrorInternalServerError(error.to_string()))?
		.query_map([request.from, request.to], |row| {
			let vehicle_state = postcard::from_bytes::<VehicleState>(&row.get::<_, Vec<u8>>(1)?)
				.map_err(|error| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Blob, Box::new(error)))?;

			Ok((row.get::<_, f64>(0)?, vehicle_state))
		})
		.and_then(|iter| iter.collect::<Result<Vec<_>, rusqlite::Error>>())
		.map_err(|error| error::ErrorInternalServerError(error.to_string()))?;

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

			Ok(
				HttpResponse::Ok()
					.content_type("text/csv")
					.body(content)
			)
		},
		"hdf5" => {
			// Generally a modified version of the csv export section
			
			// Get all sensor and valve reading names
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
			
			// Frontload iterating through the hashmap into two vectors for faster access in the loop
			let sensor_names = sensor_names
				.into_iter()
				.collect::<Vec<_>>();

			let valve_names = valve_names
				.into_iter()
				.collect::<Vec<_>>();
				
			// Temporary until I make it pass
			#[cfg(target_family = "windows")]
			let temp = &std::env::var("USERPROFILE");
			
			#[cfg(target_family = "unix")]
			let temp = &std::env::var("HOME");

			let home_path : &Path;
			match temp {
				Ok(x) => home_path = &Path::new(x),
				_ => return Err(error::ErrorInternalServerError(String::from("Could not get home path"))),
			}

			let servo_dir : &Path = &Path::new(home_path).join(".servo");
			
			// Get unique file index
			let file_index : String = EXPORT_FILE_INDEX_ATOMIC.fetch_add(1, Ordering::Relaxed).to_string();
			
			// Uneccessary since main should already make it
			if !servo_dir.is_dir() {
				return Err(error::ErrorInternalServerError(String::from("Could not get .servo path")));
			}

			// Prob can convert to just being str code. Will check later.
			let path = servo_dir.join((String::from("ExportFile") + &file_index + &String::from(".hdf5")).as_str());

			make_hdf5_file(&sensor_names, &valve_names, &vehicle_states, &path)
				.map_err(internal)?;

			let temp = std::fs::read(&path)
				.map_err(internal)?;
			let content : actix_web::web::Bytes = actix_web::web::Bytes::from(temp);

			// remove file to free up space
			let _ = std::fs::remove_file(path);

			Ok(
				HttpResponse::Ok()
					.content_type("file/hdf5")
					.body(content)
			)
		},
		_ => Err(error::ErrorBadRequest("invalid export format")),
	}
}
