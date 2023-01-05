use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Action {
	FORWARD(i32)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Target {
	
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Instruction {
	pub action: Action,
	pub target: Target,
	pub t: Duration,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Stage {
	pub name: String,
	pub sequence: Vec<Instruction>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Procedure {
	pub stages: Vec<Stage>
}
