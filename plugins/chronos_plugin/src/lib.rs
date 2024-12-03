use std::sync::{RwLock, Arc, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH, Duration};

// Import the plugin API publicly to allow the API to make calls against this plugin
pub use horizon_plugin_api::{Plugin, LoadedPlugin};

// Time configuration constants
const MINUTES_PER_HOUR: i32 = 60;
const HOURS_PER_DAY: i32 = 24;

// Global state using static
static mut TIME_STATE: Option<Arc<RwLock<TimeState>>> = None;
static mut TIME_THREAD_RUNNING: Option<Arc<AtomicBool>> = None;

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
    fn new(plugins: HashMap<&'static str, LoadedPlugin>) -> Plugin;    
}

// Implement constructor for Plugin
impl PluginConstruct for Plugin {
    fn new(_plugins: HashMap<&'static str, LoadedPlugin>) -> Plugin {
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
        println!("Chronos active: WARNING: This plugin is still in proof of concept stage and has the potential to leak resources.");
        unsafe {
            if TIME_STATE.is_none() {
                TIME_STATE = Some(Arc::new(RwLock::new(TimeState::new())));
                TIME_THREAD_RUNNING = Some(Arc::new(AtomicBool::new(true)));

                if let (Some(time_state), Some(thread_running)) = 
                    (TIME_STATE.as_ref(), TIME_THREAD_RUNNING.as_ref()) {
                    let time_state = time_state.clone();
                    let thread_running = thread_running.clone();
                    
                    std::thread::spawn(move || {
                        while thread_running.load(Ordering::SeqCst) {
                            let mut state = time_state.write().unwrap();
                            
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
        }
    }

    fn request_time(&self) -> (i32, i32) {
        unsafe {
            if let Some(time_state) = &TIME_STATE {
                let state = time_state.read().unwrap();
                (state.current_hour, state.current_minute)
            } else {
                (0, 0)
            }
        }
    }

    fn set_time(&self, hour: i32, minute: i32) {
        unsafe {
            if let Some(time_state) = &TIME_STATE {
                let mut state = time_state.write().unwrap();
                state.current_hour = hour.clamp(0, HOURS_PER_DAY - 1);
                state.current_minute = minute.clamp(0, MINUTES_PER_HOUR - 1);
            }
        }
    }

    fn set_time_mode(&self, mode: TimeMode) {
        unsafe {
            if let Some(time_state) = &TIME_STATE {
                let mut state = time_state.write().unwrap();
                state.mode = mode;
                
                if matches!(mode, TimeMode::RealTime(_)) {
                    state.start_time = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                }
            }
        }
    }

    fn get_time_mode(&self) -> TimeMode {
        unsafe {
            if let Some(time_state) = &TIME_STATE {
                let state = time_state.read().unwrap();
                state.mode
            } else {
                TimeMode::Paused
            }
        }
    }

    fn is_daytime(&self) -> bool {
        let (hour, _) = self.request_time();
        hour >= 6 && hour < 18
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