use crate::AppState;
use qingnote_core::Content;
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Placement {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    pinned: bool,
    open: bool,
    view: NoteView,
}

// Display preferences stay on this device; note content and the sync format are unchanged.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NoteView {
    collapsed: bool,
    opacity: u8,
    markdown: bool,
    font_family: String,
    font_size: u8,
}

impl Default for NoteView {
    fn default() -> Self {
        Self {
            collapsed: false,
            opacity: 100,
            markdown: true,
            font_family: "sans".into(),
            font_size: 17,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewPatch {
    collapsed: Option<bool>,
    opacity: Option<u8>,
    markdown: Option<bool>,
    width: Option<u32>,
    height: Option<u32>,
    font_family: Option<String>,
    font_size: Option<u8>,
}

const TITLE_HEIGHT: u32 = 40;

fn apply_size(window: &tauri::WebviewWindow, placement: &Placement) -> tauri::Result<()> {
    let collapsed = placement.view.collapsed;
    // A fixed-height constraint keeps the title strip small without GTK's non-resizable
    // mode restoring the old minimum height while a resize request is still queued.
    window.set_max_size(if collapsed {
        Some(tauri::LogicalSize::new(1600, TITLE_HEIGHT))
    } else {
        None
    })?;
    window.set_min_size(Some(tauri::LogicalSize::new(
        240,
        if collapsed { TITLE_HEIGHT } else { 200 },
    )))?;
    window.set_size(tauri::LogicalSize::new(
        placement.width.unwrap_or(310),
        if collapsed {
            TITLE_HEIGHT
        } else {
            placement.height.unwrap_or(330)
        },
    ))
}

#[tauri::command]
pub fn get_note_view(window: tauri::WebviewWindow, app: AppHandle) -> Result<NoteView, String> {
    let placement: Placement = app
        .state::<AppState>()
        .store
        .lock()
        .unwrap()
        .setting(window.label())
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    Ok(placement.view)
}

#[tauri::command]
pub async fn update_note_view(
    window: tauri::WebviewWindow,
    app: AppHandle,
    patch: ViewPatch,
) -> Result<NoteView, String> {
    if !window.label().starts_with("note-") {
        return Err("只能调整便签窗口".into());
    }
    if patch.opacity.is_some_and(|v| !(30..=100).contains(&v))
        || patch.width.is_some_and(|v| !(240..=1600).contains(&v))
        || patch.height.is_some_and(|v| !(200..=1200).contains(&v))
        || patch.font_size.is_some_and(|v| !(12..=32).contains(&v))
        || patch
            .font_family
            .as_ref()
            .is_some_and(|v| !["sans", "serif", "mono"].contains(&v.as_str()))
    {
        return Err("便签大小、透明度或字体设置超出允许范围".into());
    }
    remember(&app, &window.as_ref().window(), true).map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let (previous, placement) = {
        let mut store = state.store.lock().unwrap();
        let previous: Placement = store
            .setting(window.label())
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let mut placement = previous.clone();
        if let Some(value) = patch.collapsed {
            placement.view.collapsed = value;
        }
        if let Some(value) = patch.opacity {
            placement.view.opacity = value;
        }
        if let Some(value) = patch.markdown {
            placement.view.markdown = value;
        }
        if let Some(value) = patch.font_size {
            placement.view.font_size = value;
        }
        if let Some(value) = patch.font_family {
            placement.view.font_family = value;
        }
        if let Some(value) = patch.width {
            placement.width = Some(value);
        }
        if let Some(value) = patch.height {
            placement.height = Some(value);
        }
        // Save the state before native resize events arrive. Never hold SQLite across window calls.
        store
            .set_setting(window.label(), &placement)
            .map_err(|e| e.to_string())?;
        (previous, placement)
    };
    if patch.collapsed.is_some() || patch.width.is_some() || patch.height.is_some() {
        if let Err(error) = apply_size(&window, &placement) {
            let _ = state
                .store
                .lock()
                .unwrap()
                .set_setting(window.label(), &previous);
            let _ = apply_size(&window, &previous);
            return Err(error.to_string());
        }
    }
    if patch.collapsed == Some(true) && !previous.view.collapsed {
        if let Err(error) = crate::layout::avoid_collapsed(&window, placement.width.unwrap_or(310))
        {
            let _ = app.emit("layout-notice", format!("已折叠，但无法自动避让：{error}"));
        }
    }
    Ok(placement.view)
}

pub fn show_main(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_library(app: AppHandle) -> Result<(), String> {
    show_main(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_note(app: AppHandle, note_id: String) -> Result<(), String> {
    // Creating a second WebView from a synchronous IPC command can deadlock on Windows.
    open_note_window(&app, &note_id).map_err(|e| e.to_string())
}

fn open_note_window(app: &AppHandle, note_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!crate::updates::preparing(), "正在准备更新，请稍候");
    uuid::Uuid::parse_str(note_id)?;
    let label = format!("note-{note_id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    let state = app.state::<AppState>();
    state.store.lock().unwrap().note(note_id)?;
    let mut placement: Placement = state
        .store
        .lock()
        .unwrap()
        .setting(&label)?
        .unwrap_or_default();
    let builder = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("index.html?note={note_id}").into()),
    )
    .title("haonote")
    .disable_drag_drop_handler()
    .enable_clipboard_access()
    .decorations(false)
    .transparent(true)
    .inner_size(
        placement.width.unwrap_or(310) as f64,
        if placement.view.collapsed {
            TITLE_HEIGHT
        } else {
            placement.height.unwrap_or(330)
        } as f64,
    )
    .min_inner_size(
        240.0,
        if placement.view.collapsed {
            TITLE_HEIGHT as f64
        } else {
            200.0
        },
    )
    .always_on_top(placement.pinned)
    .skip_taskbar(true);
    let window = builder.build()?;
    if placement.view.collapsed {
        window.set_max_size(Some(tauri::LogicalSize::new(1600, TITLE_HEIGHT)))?;
    }
    if let (Some(x), Some(y)) = (placement.x, placement.y) {
        // If a saved monitor disappeared, let the system position the note instead.
        let monitors = app.available_monitors()?;
        if monitors.iter().any(|m| {
            let p = m.position();
            let s = m.size();
            x >= p.x && y >= p.y && x + 100 < p.x + s.width as i32 && y + 60 < p.y + s.height as i32
        }) {
            window.set_position(tauri::PhysicalPosition::new(x, y))?;
        }
    }
    placement.open = true;
    state
        .store
        .lock()
        .unwrap()
        .set_setting(&label, &placement)?;
    Ok(())
}

#[tauri::command]
pub fn close_note(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    if crate::updates::preparing() {
        return Err("正在准备更新，请稍候".into());
    }
    if window.label() == "main" {
        return window.hide().map_err(|e| e.to_string());
    }
    remember(&app, &window.as_ref().window(), false).map_err(|e| e.to_string())?;
    window.destroy().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pin_note(window: tauri::WebviewWindow, app: AppHandle, pinned: bool) -> Result<(), String> {
    window
        .set_always_on_top(pinned)
        .map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut store = state.store.lock().unwrap();
    let mut placement: Placement = store
        .setting(window.label())
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    placement.pinned = pinned;
    placement.open = true;
    store
        .set_setting(window.label(), &placement)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_pinned(window: tauri::WebviewWindow, app: AppHandle) -> Result<bool, String> {
    let placement: Placement = app
        .state::<AppState>()
        .store
        .lock()
        .unwrap()
        .setting(window.label())
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    Ok(placement.pinned)
}

#[tauri::command]
pub fn editor_ids(app: AppHandle) -> Vec<String> {
    app.webview_windows()
        .keys()
        .filter_map(|label| label.strip_prefix("note-").map(str::to_owned))
        .collect()
}

pub(crate) fn remember_position(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
) -> anyhow::Result<()> {
    let state = app.state::<AppState>();
    let mut store = state.store.lock().unwrap();
    let mut placement: Placement = store.setting(label)?.unwrap_or_default();
    placement.x = Some(x);
    placement.y = Some(y);
    store.set_setting(label, &placement)
}

fn remember(app: &AppHandle, window: &Window, open: bool) -> anyhow::Result<()> {
    let pos = window.outer_position().ok();
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = window.inner_size().ok().map(|s| s.to_logical::<u32>(scale));
    let state = app.state::<AppState>();
    let mut store = state.store.lock().unwrap();
    // Persist the user's preference. Some Linux window managers report stale ABOVE state.
    let previous: Placement = store.setting(window.label())?.unwrap_or_default();
    let placement = Placement {
        x: pos.map(|p| p.x),
        y: pos.map(|p| p.y),
        // Collapsing must not overwrite the expanded size, including queued resize events.
        width: if previous.view.collapsed {
            previous.width
        } else {
            size.map(|s| s.width).or(previous.width)
        },
        height: if previous.view.collapsed {
            previous.height
        } else {
            size.filter(|s| s.height >= 200)
                .map(|s| s.height)
                .or(previous.height)
        },
        pinned: previous.pinned,
        open,
        view: previous.view,
    };
    store.set_setting(window.label(), &placement)
}

pub fn restore_notes(app: &AppHandle) -> anyhow::Result<()> {
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let store = state.store.lock().unwrap();
        store
            .list()?
            .iter()
            .filter_map(|n| {
                let placement = store
                    .setting::<Placement>(&format!("note-{}", n.id))
                    .ok()
                    .flatten()?;
                (placement.open && !n.content.deleted && !n.content.archived)
                    .then_some(n.id.clone())
            })
            .take(30)
            .collect()
    };
    for id in ids {
        open_note_window(app, &id)?;
    }
    Ok(())
}

pub fn handle_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        if window.label() == "main" {
            // Closing Explorer hides only Explorer. Notes and the tray stay alive.
            let _ = window.hide();
        } else if window.label().starts_with("note-") {
            // Only this editor may flush and acknowledge its close request.
            let _ = window.emit_to(window.label(), "request-close", window.label());
        }
    }
    if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_))
        && window.label().starts_with("note-")
    {
        let _ = remember(window.app_handle(), window, true);
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let new = MenuItem::with_id(app, "new", "新建便签", true, None::<&str>)?;
    let library = MenuItem::with_id(app, "library", "打开便签列表", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏便签", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出haonote", true, None::<&str>)?;
    let arrange = MenuItem::with_id(app, "arrange", "一键整理便签", true, None::<&str>)?;
    let undo = MenuItem::with_id(app, "undo-arrange", "撤销整理", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&new, &library, &toggle, &arrange, &undo, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("haonote")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "arrange" | "undo-arrange" => {
                let undo = event.id.as_ref() == "undo-arrange";
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let message =
                        crate::layout::arrange(&app, undo).unwrap_or_else(|e| e.to_string());
                    let _ = app.emit("layout-notice", message);
                });
            }
            "library" => {
                let _ = show_main(app);
            }
            "new" => {
                let note = app.state::<AppState>().store.lock().unwrap().save(
                    None,
                    None,
                    Content::default(),
                );
                if let Ok(note) = note {
                    let _ = open_note_window(app, &note.id);
                    let _ = app.emit("notes-changed", ());
                }
            }
            "toggle" => {
                let windows = app.webview_windows();
                let visible = windows
                    .iter()
                    .filter(|(l, _)| l.starts_with("note-"))
                    .any(|(_, w)| w.is_visible().unwrap_or(false));
                for (label, window) in windows {
                    if label.starts_with("note-") {
                        let _ = if visible {
                            window.hide()
                        } else {
                            window.show()
                        };
                    }
                }
            }
            "quit" => {
                let _ = app.emit("request-quit", ());
            }
            _ => (),
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    if crate::updates::preparing() {
        return;
    }
    // The main window coordinates editor acknowledgements before invoking this command.
    app.exit(0);
}
