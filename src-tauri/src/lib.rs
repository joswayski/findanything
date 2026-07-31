mod actions;
mod model;
mod platform;
mod search;
mod semantic;
mod usage;

use std::sync::Arc;

use model::SearchResponse;
use search::SearchEngine;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const LAUNCHER_SHORTCUT: &str = "CommandOrControl+Shift+Space";

#[tauri::command]
async fn search(
    query: String,
    engine: tauri::State<'_, Arc<SearchEngine>>,
) -> Result<SearchResponse, String> {
    let engine = Arc::clone(engine.inner());
    tauri::async_runtime::spawn_blocking(move || engine.search(&query))
        .await
        .map_err(|error| format!("Search task failed: {error}"))
}

#[tauri::command]
async fn activate_result(
    id: String,
    query: String,
    engine: tauri::State<'_, Arc<SearchEngine>>,
) -> Result<(), String> {
    let engine = Arc::clone(engine.inner());
    tauri::async_runtime::spawn_blocking(move || engine.activate(&id, &query))
        .await
        .map_err(|error| format!("Launch task failed: {error}"))?
}

#[tauri::command]
fn hide_launcher(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Launcher window is unavailable".to_owned())?
        .hide()
        .map_err(|error| format!("Could not hide launcher: {error}"))
}

fn toggle_launcher(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("launcher-shown", ());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_launcher(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let engine = Arc::new(SearchEngine::new()?);
            let indexed_entities = engine.indexed_entities();
            app.manage(engine);
            app.global_shortcut().register(LAUNCHER_SHORTCUT)?;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.center();
                let _ = window.show();
                let _ = window.set_focus();
            }

            println!("Find Anything indexed {indexed_entities} local apps and actions");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            activate_result,
            hide_launcher
        ])
        .run(tauri::generate_context!())
        .expect("error while running Find Anything");
}
