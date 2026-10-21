//! Muster's desktop app: another client of the daemon, like the terminal UI
//! and the notch (`ARC-22`, `ARC-24`). Its Rust side holds the one
//! connection and the model (`link`); its webview (`../src`) draws.

mod commands;
pub mod contract;
mod daemon_log;
mod e2e;
mod folders;
pub mod link;
mod settings_actions;
mod sounds;
mod ticket_url;

use muster_core::Paths;
use tauri::Manager;

use crate::link::{Link, Options};

/// The link's task, aborted as the app ends (`ARC-13`).
struct LinkTask(tauri::async_runtime::JoinHandle<()>);

impl Drop for LinkTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Starts the app and blocks until it quits. The daemon, and every
/// session, keep running (`PRD-09`).
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    let result = builder
        .setup(|app| {
            // Where the terminal UI finds the daemon, from the same
            // variables (`MUSTER_RUNTIME_DIR` and the rest).
            let paths =
                Paths::resolve(|key| std::env::var_os(key), nix::unistd::getuid().as_raw())?;
            app.manage(daemon_log::DaemonLog(paths.runtime_dir.join("daemon.log")));
            let (link, task) = Link::new(Options {
                socket: paths.socket(),
                start_daemon: Some(link::starter(paths)),
            });
            app.manage(link);
            app.manage(LinkTask(tauri::async_runtime::spawn(task)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::link_open,
            commands::command,
            commands::attach,
            commands::detach,
            commands::input,
            commands::resize,
            commands::watch,
            commands::looking,
            commands::try_now,
            commands::colours,
            daemon_log::show_daemon_log,
            ticket_url::open_ticket_url,
            sounds::hear,
            folders::list_folders,
            settings_actions::restart_in_place,
            settings_actions::open_notification_settings,
            settings_actions::edit_settings_file,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("muster-desktop: {error}");
        std::process::exit(1);
    }
}
