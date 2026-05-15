mod tray;
mod window_detector;

use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::{Emitter, Manager};

#[tauri::command]
fn show_pet_menu(window: tauri::WebviewWindow) -> Result<(), String> {
    let app = window.app_handle();
    let hide = MenuItem::with_id(app, "hide_pet", "隐藏宠物", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&hide]).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            tray::create_tray(app)?;

            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_shadow(false);
            }

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_process = String::new();
                loop {
                    if let Some(name) = window_detector::get_active_process_name() {
                        if name != last_process {
                            let _ = app_handle.emit("active-process-changed", &name);
                            last_process = name;
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            });

            if let Some(win) = app.get_webview_window("settings") {
                let w = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![show_pet_menu])
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "hide_pet" {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
