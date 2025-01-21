
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use horizon_plugin_api::Pluginstate;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use  std::sync::OnceLock;
use lazy_static::lazy_static;
use socketioxide::SocketRef;

// Import the plugin API publicly to allow the API to make calls against this plugin
pub use horizon_plugin_api::{Plugin, LoadedPlugin};

// Time configuration constants
const MINUTES_PER_HOUR: i32 = 60;
const HOURS_PER_DAY: i32 = 24;

// Global state using static
//static mut TIME_STATE: Option<Arc<RwLock<TimeState>>> = None;


lazy_static! {
    static ref TIME_STATE: OnceLock<Arc<RwLock<TimeState>>> = OnceLock::new();
    static ref TIME_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
}

fn time_state() -> Arc<RwLock<TimeState>> {
    let timestate = TIME_STATE.get_or_init(|| {
        Arc::new(RwLock::new(TimeState::new()))
    });
    return timestate.clone();
}



#[derive(Clone, Copy, Debug)]
pub enum TimeMode {
    RealTime(f64),    // Multiplier relative to real time
    Virtual(f64),     // Ticks per second for virtual time
    Paused,
}

struct TimeState {
    current_hour: i32,
    current_minute: i32,
    mode: TimeMode,
    start_time: u64,
}

impl TimeState {
    fn new() -> Self {
        println!("Creating new TimeState");
        Self {
            current_hour: 6, // Start at 6 AM
            current_minute: 0,
            mode: TimeMode::Paused,
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

pub trait PluginConstruct {
    fn new(plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin;

    fn get_structs(&self) -> Vec<&str> {
        vec![]
    }

    fn player_joined(&self, socket: SocketRef) {
    }
}

// Implement constructor for Plugin
impl PluginConstruct for Plugin {
    fn new(_plugins: HashMap<String, (Pluginstate, Plugin)>) -> Plugin {
        Self::start_time_server();
        Plugin {}
    }
}

pub trait PluginAPI {
    fn start_time_server();
    fn request_time(&self) -> (i32, i32);
    fn set_time(&self, hour: i32, minute: i32);
    fn set_time_mode(&self, mode: TimeMode);
    fn get_time_mode(&self) -> TimeMode;
    fn is_daytime(&self) -> bool;
    fn get_time_of_day(&self) -> String;
}

// Implement the PluginAPI trait for Plugin
impl PluginAPI for Plugin {
    fn start_time_server() {
        
        if let Some(time_state) = TIME_STATE.get() {
            let time_state = time_state.clone();
            let thread_running = TIME_THREAD_RUNNING.load(Ordering::SeqCst);
            
            std::thread::spawn(move || {
                while thread_running {
                    let mut state = time_state.write();
                    match state.mode {
                        TimeMode::RealTime(multiplier) => {
                            let elapsed_real_seconds = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs() - state.start_time;
                                
                            let elapsed_game_seconds = (elapsed_real_seconds as f64 * multiplier) as i32;
                            let total_minutes = elapsed_game_seconds / 60;
                            
                            state.current_minute = total_minutes % MINUTES_PER_HOUR;
                            state.current_hour = (total_minutes / MINUTES_PER_HOUR) % HOURS_PER_DAY;
                        },
                        TimeMode::Virtual(ticks_per_second) => {
                            state.current_minute += 1;
                            if state.current_minute >= MINUTES_PER_HOUR {
                                state.current_minute = 0;
                                state.current_hour = (state.current_hour + 1) % HOURS_PER_DAY;
                            }
                            drop(state); // Release lock before sleeping
                            std::thread::sleep(Duration::from_secs_f64(1.0 / ticks_per_second));
                        },
                        TimeMode::Paused => {
                            drop(state); // Release lock before sleeping
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            });
        }
    }

    fn request_time(&self) -> (i32, i32) {
        let binding = time_state();
        let time_state = binding.read();
        return (time_state.current_hour, time_state.current_minute);
    }

    fn set_time(&self, hour: i32, minute: i32) {

        let binding = time_state();
        let mut time_state = binding.write();
        time_state.current_hour = hour;
        time_state.current_minute = minute;
    }


    fn get_time_mode(&self) -> TimeMode {
        let time_state = time_state().read().mode;
        match time_state {
            TimeMode::RealTime(real_time) => TimeMode::RealTime(real_time),
            TimeMode::Virtual(virt_time) => TimeMode::Virtual(virt_time),
            TimeMode::Paused => TimeMode::Paused,
        }
    }

    fn set_time_mode(&self, mode: TimeMode) {
        let binding = time_state();
        let mut time_state = binding.write();
        time_state.mode = mode;
    }

    fn is_daytime(&self) -> bool {
        let (hour, _) = self.request_time();
        match hour {
            5..=8 => true,
            9..=16 => true,
            17..=20 => true,
            _ => false,
        }
    }

    fn get_time_of_day(&self) -> String {
        let (hour, _) = self.request_time();
        match hour {
            5..=8 => "Dawn".to_string(),
            9..=16 => "Day".to_string(),
            17..=20 => "Dusk".to_string(),
            _ => "Night".to_string(),
        }
    }
}