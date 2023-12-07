use std::{time::Duration, sync::Mutex, io::{self, Write}, thread};

#[derive(Clone, Copy, Debug)]
pub struct Task {
	pub row_offset: i32
}

#[doc(hidden)]
#[allow(private_interfaces)]
pub static _TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());

/// Begins a task or subtask with a spinner.
#[macro_export]
macro_rules! task {
	($($tokens:tt)*) => {
		let mut tasks = $crate::tool::log::_TASKS.lock().unwrap();
		let message = format!($($tokens)*);

		for task in tasks.iter_mut() {
			if task.row_offset == -1 {
				task.row_offset = 1;
			} else {
				task.row_offset += 1;
			}
		}

		tasks.push($crate::tool::log::Task { row_offset: -1 });

		if tasks.len() > 1 && tasks[tasks.len() - 2].row_offset != 1 {
			print!("\x1b[{}G┣", (tasks.len() - 2) * 5 + 3);
		}

		let task_count = tasks.len();
		drop(tasks);

		print!("\n");

		if task_count > 1 {
			print!("{}", " ".repeat((task_count - 2) * 5 + 2) + "┗━ ");
		}

		print!("\x1b[33;1m-\x1b[0m {message}");
		::std::io::stdout().flush().unwrap();

		if task_count == 1 {
			::std::thread::spawn($crate::tool::log::_spin);
		}
	}
}

/// Indicates that the most recently created task has passed by replacing
/// the spinner with a green check mark.
#[macro_export]
macro_rules! pass {
	($($tokens:tt)*) => {
		$crate::tool::log::end_task!("\x1b[32;1m✔\x1b[0m", $($tokens)*);
	}
}

/// Indicates that the most recently created task has passed with a warning
/// by replacing the spinner with a yellow triangle.
#[macro_export]
macro_rules! warn {
	($($tokens:tt)*) => {
		$crate::tool::log::end_task!("\x1b[33;1m▲\x1b[0m", $($tokens)*);
	}
}

/// Indicates that the most recently created task has failed by replacing
/// the spinner with a red x.
#[macro_export]
macro_rules! fail {
	($($tokens:tt)*) => {
		$crate::tool::log::end_task!("\x1b[31;1m✘\x1b[0m", $($tokens)*);
	}
}

/// Ends the most recently created task with a custom symbol to be used
/// to replace the spinner.
#[macro_export]
macro_rules! end_task {
	($symbol:literal, $($tokens:tt)*) => {
		let mut tasks = $crate::tool::log::_TASKS.lock().unwrap();
		let message = format!($($tokens)*);

		if let Some(task) = tasks.pop() {
			// replace spinner with symbol
			print!(concat!("\x1b[s\x1b[{}A\x1b[{}G", $symbol, " \x1b[K{}"), task.row_offset, tasks.len() * 5 + 1, message);

			if task.row_offset != -1 {
				print!("\x1b[u");
			}

			::std::io::stdout().flush().unwrap();
		}

		drop(tasks);
	}
}

pub use task;
pub use pass;
pub use crate::warn;
pub use fail;
pub use end_task;

#[doc(hidden)]
pub fn _spin() {
	let mut spinner = '-';

	loop {
		let tasks = _TASKS.lock().unwrap();

		if tasks.len() == 0 {
			break;
		}

		let mut column = 1;

		for task in tasks.iter() {
			print!("\x1b[s\x1b[{}A\x1b[{column}G\x1b[33;1m{spinner}\x1b[0m\x1b[u", task.row_offset);
			column += 5;
		}

		io::stdout().flush().unwrap();

		spinner = match spinner {
			'-' => '\\',
			'\\' => '|',
			'|' => '/',
			'/' => '-',
			_ => '-',
		};

		drop(tasks);
		thread::sleep(Duration::from_millis(100));
	}
}
