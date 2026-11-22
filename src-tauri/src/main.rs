#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--service") {
        argus_lib::run_service();
    } else {
        argus_lib::run();
    }
}
