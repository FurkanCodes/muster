// No console window on Windows in release; the app is a Mac app first.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    muster_desktop::run();
}
