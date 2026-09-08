use crate::AppState;
use anyhow::{Context, Result};
use qingnote_core::{sync::Dav, Content, DavConfig, Note, SyncStatus};
use serde::Serialize;
use std::io::{Read, Write};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

type CommandResult<T> = std::result::Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn credential_entry(app: &AppHandle) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        "app.qingnote.webdav",
        &app.path().app_data_dir()?.to_string_lossy(),
    )?)
}

#[tauri::command]
pub fn list_notes(state: State<'_, AppState>) -> CommandResult<Vec<Note>> {
    state.store.lock().unwrap().list().map_err(error)
}

#[tauri::command]
pub fn save_note(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: Option<String>,
    expected: Option<String>,
    content: Content,
) -> CommandResult<Note> {
    let note = state
        .store
        .lock()
        .unwrap()
        .save(note_id.as_deref(), expected.as_deref(), content)
        .map_err(error)?;
    let _ = app.emit("notes-changed", &note.id);
    Ok(note)
}

#[tauri::command]
pub fn resolve_note(
    app: AppHandle,
    state: State<'_, AppState>,
    note_id: String,
    heads: Vec<String>,
    content: Content,
) -> CommandResult<Note> {
    let note = state
        .store
        .lock()
        .unwrap()
        .resolve(&note_id, heads, content)
        .map_err(error)?;
    let _ = app.emit("notes-changed", &note.id);
    Ok(note)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    config: Option<DavConfig>,
    has_password: bool,
    autostart: bool,
    data_directory: String,
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: State<'_, AppState>) -> CommandResult<Settings> {
    Ok(Settings {
        config: state
            .store
            .lock()
            .unwrap()
            .setting("sync_config")
            .map_err(error)?,
        has_password: state.password.lock().unwrap().is_some(),
        autostart: app.autolaunch().is_enabled().map_err(error)?,
        data_directory: app
            .path()
            .app_data_dir()
            .map_err(error)?
            .to_string_lossy()
            .into(),
    })
}

#[tauri::command]
pub async fn configure_sync(
    app: AppHandle,
    config: DavConfig,
    password: String,
    remember: bool,
) -> CommandResult<String> {
    let state = app.state::<AppState>();
    let _guard = state
        .sync_lock
        .try_lock()
        .map_err(|_| "同步正在进行，请稍后再配置".to_string())?;
    config.validate().map_err(error)?;
    let previous: Option<DavConfig> = state
        .store
        .lock()
        .unwrap()
        .setting("sync_config")
        .map_err(error)?;
    if previous.is_some_and(|p| p != config) {
        return Err("请使用已经绑定的账号和同步目录".into());
    }
    let password = if password.is_empty() {
        state
            .password
            .lock()
            .unwrap()
            .clone()
            .ok_or("请填写第三方应用密码")?
    } else {
        password
    };
    let dav = Dav::new(&config, password.clone(), state.store.clone()).map_err(error)?;
    dav.test_connection().await.map_err(error)?;
    state
        .store
        .lock()
        .unwrap()
        .bind_sync(&config)
        .map_err(error)?;
    *state.password.lock().unwrap() = Some(password.clone());
    let credential_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let entry = credential_entry(&credential_app)?;
        if remember {
            entry.set_password(&password)?;
        } else {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    })
    .await
    .map_err(error)?;
    state
        .store
        .lock()
        .unwrap()
        .set_setting("remember_password", &(remember && result.is_ok()))
        .map_err(error)?;
    Ok(match result {
        Ok(()) if remember => "连接成功，应用密码已保存到系统凭据库".into(),
        Ok(()) => "连接成功，应用密码仅在本次运行中保留".into(),
        Err(_) if remember => {
            "连接成功，但系统凭据库不可用。密码仅在本次运行中保留，重启后需重新填写".into()
        }
        Err(_) => "连接成功，但无法移除系统凭据库中的旧密码，请在系统密码管理器中检查".into(),
    })
}

#[tauri::command]
pub async fn forget_password(app: AppHandle) -> CommandResult<String> {
    let state = app.state::<AppState>();
    let _guard = state
        .sync_lock
        .try_lock()
        .map_err(|_| "同步正在进行，请稍后暂停".to_string())?;
    // Pausing must work even if the OS credential store is locked or unavailable.
    state
        .store
        .lock()
        .unwrap()
        .set_setting("remember_password", &false)
        .map_err(error)?;
    *state.password.lock().unwrap() = None;
    state.status.lock().unwrap().message = "同步已暂停，内容保存在本机".into();
    let _ = app.emit("sync-changed", ());
    let removed = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || -> Result<()> {
            match credential_entry(&app)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
    })
    .await
    .map_err(error)?;
    Ok(if removed.is_ok() {
        "已暂停同步并移除应用密码，本地便签保留".into()
    } else {
        "同步已暂停，重启后也不会自动连接。系统凭据库暂不可用，其中的旧密码需在系统密码管理器中移除"
            .into()
    })
}

pub async fn run_sync(app: &AppHandle) -> Result<SyncStatus> {
    let state = app.state::<AppState>();
    let Ok(_guard) = state.sync_lock.try_lock() else {
        return Ok(state.status.lock().unwrap().clone());
    };
    let config: Option<DavConfig> = state.store.lock().unwrap().setting("sync_config")?;
    let password = state.password.lock().unwrap().clone();
    let (Some(config), Some(password)) = (config, password) else {
        state.status.lock().unwrap().message = "内容保存在本机 · 同步未连接".into();
        return Ok(state.status.lock().unwrap().clone());
    };
    {
        let mut store = state.store.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let last: u64 = store.setting("last_attempt")?.unwrap_or(0);
        if now.saturating_sub(last) < 20 {
            return Ok(state.status.lock().unwrap().clone());
        }
        store.set_setting("last_attempt", &now)?;
    }
    {
        let mut status = state.status.lock().unwrap();
        status.running = true;
        status.message = "正在同步…".into();
    }
    let _ = app.emit("sync-changed", ());
    let result = match Dav::new(&config, password, state.store.clone()) {
        Ok(dav) => dav.sync().await,
        Err(error) => Err(error),
    };
    let mut status = state.status.lock().unwrap();
    status.running = false;
    status.pending = state
        .store
        .lock()
        .unwrap()
        .list()?
        .iter()
        .filter(|n| n.pending)
        .count();
    match result {
        Ok(outcome) => {
            if outcome.remaining == 0 {
                let time = qingnote_core::model::now();
                state
                    .store
                    .lock()
                    .unwrap()
                    .set_setting("last_sync", &time)?;
                status.last_success = Some(time);
                status.message = "已同步".into();
            } else {
                status.message = format!(
                    "本轮已同步 {} 项，还有 {} 项，将自动继续",
                    outcome.uploaded + outcome.downloaded,
                    outcome.remaining
                );
            }
        }
        Err(error) => {
            status.message = error.to_string();
        }
    }
    let result = status.clone();
    drop(status);
    let _ = app.emit("notes-changed", ());
    let _ = app.emit("sync-changed", ());
    Ok(result)
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> CommandResult<SyncStatus> {
    run_sync(&app).await.map_err(error)
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
pub async fn export_notes(app: AppHandle, plain_text: bool) -> CommandResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>> {
        let extension = if plain_text { "txt" } else { "json" };
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name(format!("qingnote-backup.{extension}"))
            .add_filter("便签备份", &[extension])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(error)
            .map_err(anyhow::Error::msg)?;
        let state = app.state::<AppState>();
        let data = if plain_text {
            state
                .store
                .lock()
                .unwrap()
                .list()?
                .iter()
                .filter(|n| !n.content.deleted)
                .map(|n| {
                    format!(
                        "{}\n{}\n{}\n",
                        n.updated_at, n.content.title, n.content.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n--------------------\n\n")
        } else {
            state.store.lock().unwrap().export()?
        };
        let mut file = tempfile::NamedTempFile::new_in(path.parent().context("无效的导出目录")?)?;
        file.write_all(data.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&path).map_err(|e| e.error)?;
        Ok(Some(path.to_string_lossy().into()))
    })
    .await
    .map_err(error)?
    .map_err(error)
}

#[tauri::command]
pub async fn import_notes(
    app: AppHandle,
    simple_sticky: Option<bool>,
) -> CommandResult<Option<usize>> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<usize>> {
        let simple_sticky = simple_sticky.unwrap_or(false);
        if simple_sticky && !app.dialog().message("将只读导入 Simple Sticky Notes 备份中的标题和纯文本正文，近似匹配颜色；已删除便签进入回收站。RTF 格式、闹钟和窗口布局不导入。重复导入跳过已有便签，不覆盖本地修改。导入后将按当前设置参与云同步。请先退出来源软件或选择其备份文件。")
            .title("导入 Simple Sticky Notes")
            .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancel).blocking_show() {
            return Ok(None);
        }
        let Some(path) = app
            .dialog()
            .file()
            .add_filter(if simple_sticky { "Simple Sticky Notes 数据库" } else { "haonote JSON 备份" },
                if simple_sticky { &["db"] } else { &["json"] })
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(error)
            .map_err(anyhow::Error::msg)?;
        if simple_sticky {
            let count = app.state::<AppState>().store.lock().unwrap().import_sticky_notes(&path)?;
            let _ = app.emit("notes-changed", ());
            return Ok(Some(count));
        }
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > 32 * 1024 * 1024 {
            anyhow::bail!("备份超过 32 MB");
        }
        let mut data = String::new();
        file.take(32 * 1024 * 1024 + 1).read_to_string(&mut data)?;
        let count = app
            .state::<AppState>()
            .store
            .lock()
            .unwrap()
            .import(&data)?;
        let _ = app.emit("notes-changed", ());
        Ok(Some(count))
    })
    .await
    .map_err(error)?
    .map_err(error)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CommandResult<()> {
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(error)
}
