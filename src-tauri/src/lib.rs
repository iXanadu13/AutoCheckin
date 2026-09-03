#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{fs, sync::Mutex};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

#[derive(Clone, Serialize, Deserialize)]
struct Step {
    action: String,
    #[serde(default = "default_locator_type")]
    locator_type: String,
    #[serde(default)]
    locator: String,
    #[serde(default)]
    value: String,
    #[serde(default = "default_wait")]
    wait_ms: u64,
}
fn default_locator_type() -> String {
    "css".to_string()
}
fn default_wait() -> u64 {
    500
}

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    login_url: String,
    #[serde(default = "default_autostart")]
    autostart: bool,
    #[serde(default)]
    steps: Vec<Step>,
}
fn default_autostart() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            login_url: String::new(),
            autostart: true,
            steps: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Status {
    completed: bool,
    last_completed_at: Option<String>,
    date: Option<String>,
}
struct AppState {
    settings: Mutex<Settings>,
    status: Mutex<Status>,
}

fn data_dir(app: &AppHandle) -> std::path::PathBuf {
    let dir = app.path().app_data_dir().expect("app data path");
    fs::create_dir_all(&dir).ok();
    dir
}
fn settings_file(app: &AppHandle) -> std::path::PathBuf {
    data_dir(app).join("settings.json")
}
fn status_file(app: &AppHandle) -> std::path::PathBuf {
    data_dir(app).join("status.json")
}
fn load_settings(app: &AppHandle) -> Settings {
    fs::read_to_string(settings_file(app))
        .ok()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}
fn load_status(app: &AppHandle) -> Status {
    fs::read_to_string(status_file(app))
        .ok()
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}
fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
fn fresh_status(status: &mut Status) {
    let current = today();
    if status.date.as_deref() != Some(current.as_str()) {
        status.completed = false;
        status.date = Some(current);
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    fs::write(
        settings_file(&app),
        serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings.clone();
    if settings.autostart {
        app.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    Ok(settings)
}

#[tauri::command]
fn get_today_status(app: AppHandle, state: State<'_, AppState>) -> Status {
    let mut status = state.status.lock().unwrap();
    fresh_status(&mut status);
    let _ = fs::write(
        status_file(&app),
        serde_json::to_string(&*status).unwrap_or_default(),
    );
    status.clone()
}

#[tauri::command]
fn mark_checkin_done(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut status = state.status.lock().unwrap();
    status.completed = true;
    status.date = Some(today());
    status.last_completed_at = Some(Local::now().format("%Y-%m-%d %H:%M").to_string());
    fs::write(
        status_file(&app),
        serde_json::to_string(&*status).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    update_tray(&app, true);
    Ok(())
}

fn tray_image(done: bool) -> tauri::image::Image<'static> {
    let color = if done {
        [54, 211, 153, 255]
    } else {
        [157, 180, 255, 255]
    };
    let mut pixels = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            if (x as i32 - 8).pow(2) + (y as i32 - 8).pow(2) < 49 {
                let i = (y * 16 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&color);
            }
        }
    }
    tauri::image::Image::new_owned(pixels, 16, 16)
}
fn update_tray(app: &AppHandle, done: bool) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_icon(Some(tray_image(done)));
        let _ = tray.set_tooltip(Some(if done {
            "AutoCheckin - Completed today"
        } else {
            "AutoCheckin - Pending"
        }));
    }
}

fn automation_script(steps: &[Step]) -> Result<String, String> {
    let steps_json = serde_json::to_string(steps).map_err(|e| e.to_string())?;
    Ok(format!(
        r#"(async () => {{
        const steps = {steps_json};
        const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
        const normalize = (value) => (value || '').replace(/\s+/g, ' ').trim().toLowerCase();
        const find = (step) => {{
            if (step.locator_type === 'xpath') return document.evaluate(step.locator, document, null, XPathResult.FIRST_ORDERED_NODE_TYPE, null).singleNodeValue;
            if (step.locator_type === 'text') return [...document.querySelectorAll('button,a,input,[role="button"],label')].find(el => normalize(el.innerText || el.value || el.getAttribute('aria-label')).includes(normalize(step.locator)));
            return document.querySelector(step.locator);
        }};
        const invokeDone = () => {{
            try {{ return window.__TAURI__?.core?.invoke('mark_checkin_done'); }} catch (_) {{
                try {{ return window.__TAURI_INTERNALS__?.invoke('mark_checkin_done'); }} catch (_) {{ return null; }}
            }}
        }};
        for (let index = 0; index < steps.length; index += 1) {{
            const step = steps[index];
            await sleep(step.wait_ms || 500);
            if (step.action === 'wait') continue;
            let element = null;
            for (let attempt = 0; attempt < 30 && !element; attempt += 1) {{ try {{ element = find(step); }} catch (_) {{}} if (!element) await sleep(500); }}
            if (!element) {{ console.warn('AutoCheckin: element not found', step); continue; }}
            element.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
            if (step.action === 'fill') {{
                const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), 'value')?.set;
                if (setter) setter.call(element, step.value || ''); else element.value = step.value || '';
                element.dispatchEvent(new Event('input', {{ bubbles: true }})); element.dispatchEvent(new Event('change', {{ bubbles: true }}));
            }} else if (step.action === 'click') {{ element.click(); if (index === steps.length - 1) await invokeDone(); }}
        }}
    }})()"#,
        steps_json = steps_json
    ))
}

fn launch_checkin_window(app: AppHandle) -> Result<(), String> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let url = url::Url::parse(&settings.login_url).map_err(|e| e.to_string())?;
    let script = automation_script(&settings.steps)?;

    if let Some(window) = app.get_webview_window("checkin-window") {
        let _ = window.destroy();
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let _window = WebviewWindowBuilder::new(&app, "checkin-window", WebviewUrl::External(url))
        .title("AutoCheckin - Automation page")
        .inner_size(1100.0, 800.0)
        .initialization_script(script)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn open_checkin_window(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = launch_checkin_window(app.clone()) {
            eprintln!("failed to open check-in window: {error}");
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            settings: Mutex::new(Settings::default()),
            status: Mutex::new(Status::default()),
        })
        .setup(|app| {
            let settings = load_settings(app.handle());
            *app.state::<AppState>().settings.lock().unwrap() = settings.clone();
            let mut status = load_status(app.handle());
            fresh_status(&mut status);
            *app.state::<AppState>().status.lock().unwrap() = status.clone();
            if settings.autostart {
                let _ = app.autolaunch().enable();
            }
            update_tray(app.handle(), status.completed);
            let menu = MenuBuilder::new(app)
                .item(&MenuItemBuilder::with_id("open", "Open main window").build(app)?)
                .item(&MenuItemBuilder::with_id("quit", "Exit").build(app)?)
                .build()?;
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_image(status.completed))
                .menu(&menu)
                .tooltip(if status.completed {
                    "AutoCheckin - Completed today"
                } else {
                    "AutoCheckin - Pending"
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let window_for_close = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_for_close.hide();
                    }
                });
            }
            if settings.autostart
                && !status.completed
                && !settings.login_url.is_empty()
                && !settings.steps.is_empty()
            {
                // 取消启动时自动打卡
                // let handle = app.handle().clone();
                // std::thread::spawn(move || {
                //     std::thread::sleep(std::time::Duration::from_secs(3));
                //     let _ = launch_checkin_window(handle);
                // });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_today_status,
            mark_checkin_done,
            open_checkin_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
