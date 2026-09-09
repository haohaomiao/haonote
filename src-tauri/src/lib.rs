mod commands;
mod layout;
mod updates;
mod windows;

use qingnote_core::{sync::SharedStore, Store, SyncStatus};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub struct AppState {
    store: SharedStore,
    password: Mutex<Option<String>>,
    sync_lock: tokio::sync::Mutex<()>,
    status: Mutex<SyncStatus>,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = windows::show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.manage(layout::LayoutState::default());
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            }
            let store = Store::open(directory.join("qingnote.sqlite3"))?;
            let last_success = store.setting::<String>("last_sync")?;
            app.manage(AppState {
                store: Arc::new(Mutex::new(store)),
                password: Mutex::new(None),
                sync_lock: tokio::sync::Mutex::new(()),
                status: Mutex::new(SyncStatus {
                    last_success,
                    message: "内容保存在本机".into(),
                    ..Default::default()
                }),
            });
            windows::setup_tray(app.handle())?;
            windows::restore_notes(app.handle())?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Credential-store access can block; never run it on the UI thread.
                let app = handle.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    let configured = app
                        .state::<AppState>()
                        .store
                        .lock()
                        .unwrap()
                        .setting::<bool>("remember_password")
                        .ok()
                        .flatten()
                        .unwrap_or(false);
                    if !configured {
                        return;
                    }
                    if let Ok(entry) = commands::credential_entry(&app) {
                        if let Ok(password) = entry.get_password() {
                            *app.state::<AppState>().password.lock().unwrap() = Some(password);
                        }
                    }
                })
                .await;
                loop {
                    let _ = commands::run_sync(&handle).await;
                    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                }
            });
            Ok(())
        })
        .on_window_event(windows::handle_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_notes,
            commands::save_note,
            commands::resolve_note,
            commands::get_settings,
            commands::configure_sync,
            commands::sync_now,
            commands::sync_status,
            commands::export_notes,
            commands::import_notes,
            commands::set_autostart,
            commands::forget_password,
            windows::open_note,
            windows::close_note,
            windows::pin_note,
            windows::open_library,
            windows::quit_app,
            windows::get_pinned,
            windows::get_note_view,
            windows::update_note_view,
            windows::editor_ids,
            layout::arrange_notes,
            updates::update_support,
            updates::open_releases,
            updates::prepare_update
        ])
        .build(tauri::generate_context!())
        .expect("无法启动haonote")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { ref api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            if let tauri::RunEvent::Exit = event {
                let _ = app.emit("app-exit", ());
            }
        });
}
