// Windows release builds must not spawn a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vapor_app_lib::run()
}
