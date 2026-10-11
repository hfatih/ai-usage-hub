#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|info| {
        if let Some(appdata) = std::env::var_os("LOCALAPPDATA") {
            let log_path = std::path::Path::new(&appdata).join("AI Usage Hub").join("crash.log");
            let _ = std::fs::write(log_path, format!("{:?}", info));
        }
    }));
    ai_usage_hub_lib::run();
}
