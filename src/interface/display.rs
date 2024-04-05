use common::comm::CompositeValveState;
use tower_http::body::Full;
use crate::server::Shared;
use std::{collections::{ HashMap, HashSet }, error::Error, fmt, hash::Hash, io::{self, Stdout, Write}, ops::Div, time::{ Duration, Instant }, vec::Vec};
use sysinfo::{System, SystemExt, CpuExt};

use tokio::time::sleep;
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::string::String;
use common::comm::{Measurement, ValveState, Unit};

fn get_state_style(state : ValveState) -> Style {
	match state {
		ValveState::Undetermined => Style::default().fg(Color::White).bg(Color::DarkGray).bold(),
		ValveState::Disconnected => Style::default().fg(Color::Black).bg(Color::Gray).bold(),
		ValveState::Open => Style::default().fg(Color::White).bg(Color::Green).bold(),
		ValveState::Closed => Style::default().fg(Color::White).bg(Color::Red).bold(),
		ValveState::Fault => Style::default().fg(Color::White).bg(Color::Blue).bold(),
		_ => Style::default().fg(Color::White).bg(Color::Magenta).bold(),
	}
}

fn get_full_row_style(state : ValveState) -> Style {
	match state {
		ValveState::Undetermined => Style::default().fg(Color::White).bg(Color::DarkGray),
		ValveState::Fault => Style::default().fg(Color::White).bg(Color::LightRed),
		ValveState::Disconnected => Style::default().fg(Color::Gray).bg(Color::DarkGray),
		_ => Style::default().fg(Color::White).bg(Color::Black),
	}
}

struct NamedValue<T : Clone> {
    name : String,
    value : T,
}

impl<T : Clone> NamedValue<T> {
    fn new(new_name : String, new_value : T) -> NamedValue<T> {
        NamedValue {
            name : new_name,
            value : new_value,
        }
    }
}

/// A fast and stable ordered vector of objects with a corresponding string key stored in a hashmap
/// 
/// Used in GUI to hold items grabbed from a hashmap / hashset for a constant ordering when iterated through
/// and holding historic data
struct StringLookupVector<T : Clone> {
    lookup : HashMap<String, usize>,
    vector : Vec<NamedValue<T>>,
}


struct StringLookupVectorIter<'a, T : Clone> {
    reference : &'a StringLookupVector<T>,
    index : usize,
}

impl<'a, T : Clone> Iterator for StringLookupVectorIter<'a, T> {
    // we will be counting with usize
    type Item = &'a NamedValue<T>;

    // next() is the only required method
    fn next(&mut self) -> Option<Self::Item> {

        // Check to see if we've finished counting or not.
        let out : Option<Self::Item>;
        if self.index < self.reference.vector.len() {
            out = Some(self.reference.vector.get(self.index).unwrap())
        } else {
            out = None
        }
        // Increment the index
        self.index += 1;

        return out;
    }
}

impl<T : Clone> StringLookupVector<T> {
    const DEFAULT_CAPACITY : usize = 8;
    fn len(&self) -> usize {
        self.vector.len()
    }
    /// Creates a new StringLookupVector with a specified capacity
    fn with_capacity(capacity : usize) -> StringLookupVector<T> {
        StringLookupVector { 
            lookup : HashMap::<String, usize>::with_capacity(capacity), 
            vector : Vec::<NamedValue<T>>::with_capacity(capacity), 
        }
    }
    /// Creates a new StringLookupVector with default capacity
    fn new() -> StringLookupVector<T> {
        StringLookupVector::with_capacity(StringLookupVector::<T>::DEFAULT_CAPACITY)
    }
    /// Checks if a key is contained within the StringLookupVector
    fn contains_key(&self, key : &String) -> bool {
        self.lookup.contains_key(key)
    }
    /// Returns the index of a key in the vector
    fn index_of(&self, key : &String) -> Option<usize> {
        self.lookup.get(key).copied()
    }
    
    /// Returns true if the object was added, and false if it was replaced
    fn add(&mut self, name : &String, value : T) {
        if self.contains_key(name) {
            self.vector[self.lookup[name]].value = value;
            return;
        }
        self.lookup.insert(name.clone(), self.vector.len());
        self.vector.push(NamedValue::new(name.clone(), value));
    }
    fn remove(&mut self, key : &String) {
        if self.contains_key(key) {
            self.vector.remove(self.lookup[key]);
            self.lookup.remove(key);
        }
    }

    /// Inefficient right now but it'll have to do
    /// 
    /// This shouldn't be called every render anyways / should be a manual keybind
    fn sort_by_name(&mut self) {
        self.vector.sort_unstable_by_key(|x| x.name.to_string());
        for i in 0..self.vector.len() {
            *self.lookup.get_mut(&self.vector[i].name).unwrap() = 1; // Key has to exist by the nature of this structure
        }
    }

    /// Gets a mutable reference to the item with the given key.
    /// Panics if the key is not valid
    fn get(&mut self, key : &String) -> Option<&NamedValue<T>> {
        let index = self.lookup.get(key);
        match index {
            Some(x) => self.vector.get(x.clone()),
            None => None
        }
    }
    /// Gets a mutable reference to the item with the given index in the vector.
    /// Panics if the key is not valid
    fn get_from_index(&mut self, index : usize) -> Option<&NamedValue<T>> {
        self.vector.get(index)
    }
    /// Gets a mutable reference to the item with the given key.
    /// Panics if the key is not valid
    fn get_mut(&mut self, key : &String) -> Option<&mut NamedValue<T>> {
        let index = self.lookup.get(key);
        match index {
            Some(x) => self.vector.get_mut(x.clone()),
            None => None
        }
    }
    /// Gets a mutable reference to the item with the given index in the vector.
    /// Panics if the key is not valid
    fn get_mut_from_index(&mut self, index : usize) -> Option<&mut NamedValue<T>> {
        self.vector.get_mut(index)
    }

    fn iter(&self) -> StringLookupVectorIter<T> {
        StringLookupVectorIter::<T> {
            reference : self,
            index : 0,
        }
    }
}


#[derive(Clone)]
struct BasicValveDatapoint {
    name : String,
    value : f64,
    state : CompositeValveState,
}

#[derive(Clone)]
struct InvalidNameValveDatapoint {
    data : BasicValveDatapoint,
    rolling_average : f64,
}

#[derive(Clone)]
struct FullValveDatapoint {
    voltage : f64,
    current : f64,
    rolling_voltage_average : f64,
    rolling_current_average : f64,
    state : CompositeValveState,
}

#[derive(Clone)]
struct SensorDatapoint {
    measurement : Measurement,
    rolling_average : f64
}

impl SensorDatapoint {
    fn new(first_measurement : &Measurement) -> SensorDatapoint {
        SensorDatapoint { measurement : first_measurement.clone(), rolling_average : first_measurement.value }
    }
}

struct GuiData {
    sensors : StringLookupVector<SensorDatapoint>,
    valves : StringLookupVector<FullValveDatapoint>,
    poorly_named_valves : StringLookupVector<BasicValveDatapoint>,
}

impl GuiData {
    fn new() -> GuiData {
        GuiData {
            sensors : StringLookupVector::<SensorDatapoint>::new(),
            valves : StringLookupVector::<FullValveDatapoint>::new(),
            poorly_named_valves : StringLookupVector::<BasicValveDatapoint>::new(),
        }
    }
}

async fn update_information(gui_data : &mut GuiData, shared : &Shared) {
	// display sensor data
	let vehicle_state = shared.vehicle.0
		.lock()
		.await
		.clone();

	let sensor_readings = vehicle_state.sensor_readings
		.iter()
		.collect::<Vec<_>>();

	let valve_states = vehicle_state.valve_states
		.iter()
		.collect::<Vec<_>>();

	let mut sort_needed = false;
	for (name, value) in valve_states {
		match gui_data.valves.get_mut(name) {
			Some(x) => x.value.state = value.clone(),
			None => {
				gui_data.valves.add(name, FullValveDatapoint { voltage : 0.0, current : 0.0, rolling_voltage_average : 0.0, rolling_current_average : 0.0, state : value.clone() });
				sort_needed = true;
			},
		}
	}
	if sort_needed {
		gui_data.valves.sort_by_name();
	}
	const CURRENT_SUFFIX : &str = "_I"; 
	const VOLTAGE_SUFFIX : &str = "_V"; 
	sort_needed = true;
	for (name, value) in sensor_readings {
		if name.len() > 2 {
			if name.ends_with(CURRENT_SUFFIX) {
				let mut real_name = name.clone();
				let _ = real_name.split_off(real_name.len() - 2);
				match gui_data.valves.get_mut(&real_name) {
					Some(x) => {
						x.value.current = value.value;
						x.value.rolling_current_average *= 0.8;
						x.value.rolling_current_average += 0.2 * value.value;
						continue;
					}
					None => {},
				}
			} else if name.ends_with(VOLTAGE_SUFFIX) {
				let mut real_name = name.clone();
				let _ = real_name.split_off(real_name.len() - 2);
				match gui_data.valves.get_mut(&real_name) {
					Some(x) => {
						x.value.voltage = value.value;
						x.value.rolling_voltage_average *= 0.8;
						x.value.rolling_voltage_average += 0.2 * value.value;
						continue;
					}
					None => {},
				}
			}
		}
		match gui_data.sensors.get_mut(name) {
			Some(x) =>  {
				x.value.measurement = value.clone();
				x.value.rolling_average *= 0.8;
				x.value.rolling_average += 0.2 * value.value.clone();
			},
			None => {
				gui_data.sensors.add(name, SensorDatapoint { measurement : value.clone(), rolling_average : value.value.clone() });
				sort_needed = true;
			},
		}
	}
	if sort_needed {
		gui_data.sensors.sort_by_name();
	}
}

fn display_round(terminal : &mut Terminal<CrosstermBackend<Stdout>>, gui_data : &mut GuiData, tick_rate : Duration, last_tick : &mut Instant) -> bool {
	let _ = terminal.draw(|f| servo_ui(f, 0, gui_data));

	let poll_res = crossterm::event::poll(Duration::from_millis(0));

	if poll_res.is_err() {
		println!("Input polling failed : ");
		println!("{}", poll_res.unwrap_err());
		return false;
	}
	if poll_res.unwrap() {
		let read_res = event::read();
		if read_res.is_err() {
			println!("Input reading failed : ");
			println!("{}", read_res.unwrap_err());
			return false;
		}
		if let Event::Key(key) = read_res.unwrap() {
			if let KeyCode::Char('q') = key.code {
				return false;
			}
		}
	}
	if last_tick.elapsed() >= tick_rate {
		last_tick.clone_from(&Instant::now());
	}
	return true;
}


fn restore_terminal(terminal : &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), Box<dyn Error>> {
    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    //if let Err(err) = res {
    //    println!("{err:?}");
    //}

    Ok(())
}

/// Continuously refreshes and updates the display with new data.
pub async fn display(shared: Shared) {

	// setup terminal
	// This is absolutely garbage code, but backend doesn't implement copy so I can't make this a simple error catch function with one print statement without the send requirement failing
    let res = enable_raw_mode();
	if res.is_err() {
		println!("Display initialization failed : ");
		println!("{}", res.unwrap_err());
		return;
	}
	let _ = res.unwrap();

    let mut stdout = io::stdout();
    let res = execute!(stdout, EnterAlternateScreen, EnableMouseCapture);
	if res.is_err() {
		println!("Display initialization failed : ");
		println!("{}", res.unwrap_err());
		return;
	}
	let _ = res.unwrap();
	
    let backend = CrosstermBackend::new(stdout);
    let res = Terminal::new(backend);
	if res.is_err() {
		println!("Display initialization failed : ");
		println!("{}", res.unwrap_err());
		return;
	}

	let mut terminal = res.unwrap();

	let mut system = System::new_all();
	let hostname = system.host_name()
		.unwrap_or("\x1b[33mnone\x1b[0m".to_owned());

    // create gui_data and run the gui
    let tick_rate = Duration::from_millis(100);
    let mut gui_data : GuiData = GuiData::new();
	let mut last_tick = Instant::now();
    loop {
		// display system statistics
		system.refresh_cpu();
		system.refresh_memory();

		let cpu_usage = system
			.cpus()
			.iter()
			.fold(0.0, |util, cpu| util + cpu.cpu_usage())
			.div(system.cpus().len() as f32);
		let memory_usage = system.used_memory() as f32 / system.total_memory() as f32 * 100.0;
		update_information(&mut gui_data, &shared).await;
        if !display_round(&mut terminal, &mut gui_data, tick_rate, &mut last_tick) {
			break;
		}
		sleep(tick_rate).await;
    }

	let res = restore_terminal(&mut terminal);
	if res.is_err() {
		println!("Terminal restoration failed : ");
		println!("{}", res.unwrap_err());
		return;
	}
	return;
	// TODO : make the entire application quit

	// only kept around to copy later
	loop {




		//system_container.write_line(0, &format!("CPU Usage: \x1b[1m{cpu_usage:.1}%\x1b[0m"));
		//system_container.write_line(1, &format!("Memory Usage: \x1b[1m{memory_usage:.1}%\x1b[0m"));
		//system_container.write_line(2, &format!("Host name: \x1b[1m{hostname}\x1b[0m"));
		
		// display network statistics

		// display sensor data
		let vehicle_state = shared.vehicle.0
			.lock()
			.await
			.clone();

		let mut sensor_readings = vehicle_state.sensor_readings
			.iter()
			.collect::<Vec<_>>();

		let mut valve_states = vehicle_state.valve_states
			.iter()
			.collect::<Vec<_>>();
	}
}


fn servo_ui(f: &mut Frame, selected_tab : usize, gui_data: &mut GuiData) {
    let chunks: std::rc::Rc<[Rect]> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Fill(1)])
        .split(f.size());

    let tab_menu = Tabs::new(vec!["Home", "Charts", "System"])
        .block(Block::default().title("Tabs").borders(Borders::ALL))
        .style(Style::default().white())
        .highlight_style(Style::default().yellow())
        .select(selected_tab)
        .divider(symbols::line::VERTICAL);

    
    f.render_widget(tab_menu, chunks[0]);

    match selected_tab {
        0 => home_menu(f, chunks[1], gui_data),
        _ => bad_tab(f, chunks[1])
    };
}

fn bad_tab(f: &mut Frame, area : Rect) {
    return;
}
fn home_menu(f: &mut Frame, area : Rect, gui_data: &mut GuiData) {

    let horizontal  = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(80), Constraint::Length(60)])
        .split(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(3, 4), Constraint::Ratio(1, 4)])
        .split(horizontal[0]);

    draw_valves(f, horizontal[1], gui_data);

    draw_sensors(f, horizontal[2], gui_data);

    //draw_bar_with_group_labels(f, chunks[0]);
    //draw_horizontal_bars(f, chunks[1]);
}

fn draw_valves(f: &mut Frame, area : Rect, gui_data: &mut GuiData) {
    let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Ratio(3, 4), Constraint::Ratio(1, 4)])
    .split(area);

	let full_valves : &mut StringLookupVector<FullValveDatapoint> = &mut gui_data.valves;

    // Properly named valve state rendering
    {
        let mut rows : Vec<Row> = Vec::<Row>::with_capacity(full_valves.len());
        for pair in full_valves.iter() {
			let name = &pair.name;
			let value = &pair.value;
            let normal_style = get_full_row_style(value.state.actual);

            let d_v = value.voltage - value.rolling_voltage_average;
            let d_v_style : Style;
            if d_v.abs() < 0.05 {
                d_v_style = normal_style;
            } else {
                if d_v > 0.0 {
                    d_v_style = normal_style.fg(Color::Green);
                } else {
                    d_v_style = normal_style.fg(Color::Red);
                }
            }

            let d_i: f64 = value.current - value.rolling_current_average;
            let d_i_style : Style;
            if d_i.abs() < 0.05 {
                d_i_style = normal_style;
            } else {
                if d_i > 0.0 {
                    d_i_style = normal_style.fg(Color::Green);
                } else {
                    d_i_style = normal_style.fg(Color::Red);
                }
            }
            rows.push(Row::new(vec![
                Cell::from(Span::from(name.clone()).style(normal_style.fg(Color::LightYellow)).bold()),
                Cell::from(Span::from(format!("{:.2}", value.voltage)).to_right_aligned_line()), 
                Cell::from(Span::from(format!("{:+.3}", d_v)).to_right_aligned_line()).style(d_v_style),
                Cell::from(Span::from(format!("{:.3}", value.current)).to_right_aligned_line()),
                Cell::from(Span::from(format!("{:+.3}", d_i)).to_right_aligned_line()).style(d_i_style),
                Cell::from(Span::from(format!("{}", value.state.actual)).to_centered_line()).style(get_state_style(value.state.actual)),
                Cell::from(Span::from(format!("{}", value.state.commanded)).to_centered_line()).style(get_state_style(value.state.commanded))
            ]).style(normal_style));
        }

        let widths = [
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(12),
            Constraint::Length(12),
        ];

        let valve_table: Table<'_> = Table::new(rows, widths)
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black))
        // It has an optional header, which is simply a Row always visible at the top.
        .header(
            Row::new(vec![Line::from("Name"), Span::from("Voltage").to_right_aligned_line(), Line::from(""), Span::from("Current").to_right_aligned_line(), Line::from(""), Span::from("State").to_centered_line(), Span::from("Commanded").to_centered_line()])
                .style(Style::new().bold())
                // To add space between the header and the rest of the rows, specify the margin
                .bottom_margin(1),
        )
        // As any other widget, a Table can be wrapped in a Block.
        .block(Block::default().title("Valves").borders(Borders::ALL))
        // The selected row and its content can also be styled.
        .highlight_style(Style::new().reversed())
        // ...and potentially show a symbol in front of the selection.
        .highlight_symbol(">>");


        f.render_widget(valve_table, chunks[0]);
    }
}

fn draw_sensors(f: &mut Frame, area : Rect, gui_data: &mut GuiData) {

    let full_sensors : &mut StringLookupVector<SensorDatapoint> = &mut gui_data.sensors;
    
    // Time to actually make the table
    
    let normal_style = Style::default().fg(Color::LightYellow).bg(Color::Black);
    let data_style = normal_style.fg(Color::White);

    let mut rows : Vec<Row> = Vec::<Row>::with_capacity(full_sensors.len());

    for name_datapoint_pair in full_sensors.iter() {
        let name : &String = &name_datapoint_pair.name;
        let datapoint : &SensorDatapoint = &name_datapoint_pair.value;

        let d_v = datapoint.measurement.value - datapoint.rolling_average;
        let d_v_style : Style;

        let value_magnitude_min : f64 = 1.0;
        let value_magnitude : f64;
        if datapoint.rolling_average.abs() > value_magnitude_min {
            value_magnitude = datapoint.rolling_average.abs()
        } else {
            value_magnitude = value_magnitude_min
        }
        
        // If the change is > 1% the rolling averages value, then it's considered significant enough to highlight.
        // Since sensors have a bigger potential range, a flat delta threshold is a bad idea as it would require configuration.
        if d_v.abs() / value_magnitude < 0.01 {
            d_v_style = data_style;
        } else {
            if d_v > 0.0 {
                d_v_style = normal_style.fg(Color::Green);
            } else {
                d_v_style = normal_style.fg(Color::Red);
            }
        }

        rows.push(Row::new(vec![
            Cell::from(Span::from(name.clone()).style(normal_style).bold()),
            Cell::from(Span::from(format!("{:.3}", datapoint.measurement.value)).to_right_aligned_line().style(data_style)), 
            Cell::from(Span::from(format!("{}", datapoint.measurement.unit)).to_left_aligned_line().style(data_style.fg(Color::Gray))), 
            Cell::from(Span::from(format!("{:+.3}", d_v)).to_left_aligned_line()).style(d_v_style),
        ]).style(normal_style));
    }

    let widths = [
        Constraint::Min(16),
        Constraint::Min(12),
        Constraint::Length(7),
        Constraint::Min(14)
    ];

    let sensor_table: Table<'_> = Table::new(rows, widths)
        .style(normal_style)
        // It has an optional header, which is simply a Row always visible at the top.
        .header(
            Row::new(vec![Line::from("Name"), Span::from("Value").to_right_aligned_line(), Span::from("Unit").to_centered_line(), Span::from("Rolling Change").to_centered_line()])
                .style(Style::new().bold())
                // To add space between the header and the rest of the rows, specify the margin
                .bottom_margin(1),
        )
        // As any other widget, a Table can be wrapped in a Block.
        .block(Block::default().title("Sensors").borders(Borders::ALL))
        // The selected row and its content can also be styled.
        .highlight_style(Style::new().reversed())
        // ...and potentially show a symbol in front of the selection.
        .highlight_symbol(">>");


        f.render_widget(sensor_table, area);
}