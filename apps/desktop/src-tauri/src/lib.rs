//! Muster's desktop app: another client of the daemon, like the terminal UI
//! and the notch (`ARC-22`, `ARC-24`). Its Rust side holds the one
//! connection and the model; its webview (`../src`) draws.

mod commands;
pub mod contract;

/// Starts the app and blocks until it quits.
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    let result = builder
        .invoke_handler(tauri::generate_handler![
            commands::link_open,
            commands::command,
            commands::attach,
            commands::detach,
            commands::input,
            commands::resize,
            commands::watch,
            commands::looking,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("muster-desktop: {error}");
        std::process::exit(1);
    }
}
