//! Where a project is: the Mac's own folder panel, which the person
//! chooses a folder in (`PRODUCT.md` §6a); and, in builds for the
//! end-to-end tests only, whose driver cannot reach a panel, the folder
//! browser that stands in for it (`browse`). A shipped app reads no folder
//! itself (`ARC-24`).

use std::path::PathBuf;

use serde::Serialize;
use tauri_plugin_dialog::DialogExt;
use ts_rs::TS;

#[cfg(any(feature = "e2e", test))]
pub(crate) mod browse;

/// What the Mac's folder panel gave back, the person adding a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PickedFolder {
    /// The folder the person chose.
    Chosen { path: PathBuf },
    /// The person closed the panel without choosing.
    Cancelled,
    /// No panel here: a build for the end-to-end tests, whose driver cannot
    /// reach one, where the folder browser stands in.
    Unavailable,
}

/// The Mac's own folder panel, as a sheet on the window, opened at the
/// home folder and titled `title` (to add a project, or to clone one into
/// a folder: `PRODUCT.md` §6a). The webview is given only this: it never
/// opens a panel, or reads a folder, itself.
#[tauri::command]
pub(crate) async fn pick_folder(window: tauri::Window, title: Option<String>) -> PickedFolder {
    if cfg!(feature = "e2e") {
        return PickedFolder::Unavailable;
    }
    let (sent, chosen) = tokio::sync::oneshot::channel();
    let mut panel = window
        .dialog()
        .file()
        .set_title(title.unwrap_or_else(|| "Add a project".to_owned()))
        .set_parent(&window);
    if let Some(home) = std::env::var_os("HOME") {
        panel = panel.set_directory(home);
    }
    panel.pick_folder(move |path| {
        let _ = sent.send(path);
    });
    match chosen.await {
        Ok(Some(path)) => path
            .into_path()
            .map_or(PickedFolder::Cancelled, |path| PickedFolder::Chosen { path }),
        Ok(None) | Err(_) => PickedFolder::Cancelled,
    }
}
