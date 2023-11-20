use std::{time::Duration, sync::Mutex, io::{self, Write}, thread};
use once_cell::sync::Lazy;

struct Task {
	subtasks: i32,
	subtask_running: bool,
}

static TASK: Lazy<Mutex<Option<Task>>> = Lazy::new(|| Mutex::new(None));

#[macro_export]
macro_rules! task {
	($($tokens:tt)*) => {
		crate::tool::task::_task(format!($($tokens)*));
	}
}

#[macro_export]
macro_rules! subtask {
	($($tokens:tt)*) => {
		crate::tool::task::_subtask(format!($($tokens)*));
	}
}

#[macro_export]
macro_rules! pass {
	($($tokens:tt)*) => {
		crate::tool::task::_pass(format!($($tokens)*));
	}
}

#[macro_export]
macro_rules! fail {
	($($tokens:tt)*) => {
		crate::tool::task::_fail(format!($($tokens)*));
	}
}

pub use task;
pub use subtask;
pub use pass;
pub use fail;

pub fn _task(message: String) {
	let mut task = TASK.lock().unwrap();

	let task_running = task.is_some();
	*task = Some(Task { subtasks: 0, subtask_running: false });

	print!("\x1b[33;1m-\x1b[0m {message}");

	if !task_running {
		// spawn thread which automatically stops when the task is None
		thread::spawn(move || {
			let mut spinner = '-';

			loop {
				if let Some(task) = TASK.lock().unwrap().as_ref() {
					// save cursor position
					print!("\x1b[s");

					// replace subtask spinner
					if task.subtask_running && task.subtasks > 0 {
						print!("\x1b[6G\x1b[33;1m{spinner}\x1b[0m");
					}

					let line_offset = if task.subtasks > 0 { task.subtasks } else { -1 };

					// replace task spinner and restore cursor
					print!("\x1b[{line_offset}A\x1b[0G\x1b[33;1m{spinner}\x1b[0m\x1b[u");
					io::stdout().flush().unwrap();

					// lock is dropped here, so sleep does not block
				} else {
					break;
				}

				spinner = match spinner {
					'-' => '\\',
					'\\' => '|',
					'|' => '/',
					'/' => '-',
					_ => '-',
				};

				thread::sleep(Duration::from_millis(100));
			}
		});
	}
}

pub fn _pass(message: String) {
	let mut task_mutex = TASK.lock().unwrap();

	if let Some(task) = task_mutex.as_mut() {
		// replace subtask spinner with check mark
		if task.subtask_running && task.subtasks > 0 {
			print!("\x1b[6G\x1b[32;1m✔\x1b[0m \x1b[0K{message}");
			task.subtask_running = false;
		} else {
			// replace task spinner with check mark
			println!("\x1b[s\x1b[{}A\x1b[0G\x1b[32;1m✔\x1b[0m \x1b[K{message}\x1b[u", task.subtasks);
			*task_mutex = None;
		}
	}
}

pub fn _fail(message: String) {
	let mut task_mutex = TASK.lock().unwrap();

	if let Some(task) = task_mutex.as_mut() {
		// replace subtask spinner with check mark
		if task.subtask_running && task.subtasks > 0 {
			print!("\x1b[6G\x1b[31;1m✘\x1b[0m \x1b[0K{message}");
			task.subtask_running = false;
		} else {
			// replace task spinner with check mark
			println!("\x1b[s\x1b[{}A\x1b[0G\x1b[31;1m✘\x1b[0m \x1b[K{message}\x1b[u", task.subtasks);
			*task_mutex = None;
		}
	}
}

pub fn _subtask(message: String) {
	if let Some(task) = TASK.lock().unwrap().as_mut() {
		if task.subtasks > 0 {
			print!("\x1b[s\x1b[-1A\x1b[3G┣━\x1b[u");
		}

		print!("\n  ┗━ \x1b[33;1m-\x1b[0m {message}");

		task.subtask_running = true;
		task.subtasks += 1;

		io::stdout().flush().unwrap();
	}
}
