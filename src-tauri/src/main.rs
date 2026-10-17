#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|info| {
        if let Some(dir) = ai_usage_hub_lib::platform::log_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("crash.log"), format!("{:?}", info));
        }
    }));
    ai_usage_hub_lib::run();
}
