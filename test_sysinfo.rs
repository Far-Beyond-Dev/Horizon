use sysinfo::{System, Pid, ProcessExt, SystemExt};
use tracing::info;

fn main() {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // Get current process ID
    let current_pid = std::process::id();
    info!("Current PID: {}", current_pid);
    
    // Try to get process info
    if let Some(proc) = sys.process(Pid::from(current_pid as usize)) {
        info!("Memory usage: {} bytes", proc.memory());
        info!("Memory usage: {} MB", proc.memory() / 1024 / 1024);
    } else {
        info!("Could not find current process");
    }
}
