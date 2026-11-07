// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--log") {
        return n230_control_lib::log(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60));
    }
    if args.get(1).is_some_and(|a| a == "--scan") {
        return n230_control_lib::scan(args.get(2).and_then(|s| s.parse().ok()));
    }
    if args.get(1).is_some_and(|a| a == "--paired") {
        return n230_control_lib::paired();
    }
    n230_control_lib::run()
}
