use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, WebviewWindow};

// Prevent windows from opening/closing between the save barrier and installation.
static PREPARING: AtomicBool = AtomicBool::new(false);

pub fn preparing() -> bool {
    PREPARING.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn open_releases() -> Result<(), String> {
    tauri_plugin_opener::open_url(
        "https://github.com/haohaomiao/haonote/releases",
        None::<&str>,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn prepare_update(window: WebviewWindow, preparing: bool) -> Result<(), String> {
    if window.label() != "main" {
        return Err("只有 Explorer 可以安装更新".into());
    }
    PREPARING.store(preparing, Ordering::SeqCst);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSupport {
    version: String,
    enabled: bool,
    reason: &'static str,
}

#[tauri::command]
pub fn update_support(app: AppHandle) -> UpdateSupport {
    let configured = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|c| c.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty());
    let reason = if cfg!(debug_assertions) || app.config().identifier != "app.qingnote.desktop" {
        "开发版不安装正式版更新"
    } else if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none() {
        "Linux deb 请下载新版软件包安装；应用内更新支持 AppImage"
    } else if !cfg!(any(target_os = "windows", target_os = "linux")) {
        "此平台尚未提供更新包"
    } else if !configured {
        "此构建尚未配置更新签名公钥"
    } else {
        ""
    };
    UpdateSupport {
        version: app.package_info().version.to_string(),
        enabled: reason.is_empty(),
        reason,
    }
}
