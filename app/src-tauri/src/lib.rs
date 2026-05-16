mod config_manager;
mod system_monitor;
mod tray;
mod window_detector;

use tauri::menu::{Menu, MenuItem};
use tauri::{Manager, PhysicalPosition};

#[tauri::command]
fn show_pet_menu(window: tauri::WebviewWindow) -> Result<(), String> {
    let app = window.app_handle();
    let hide = MenuItem::with_id(app, "hide_pet", "隐藏宠物", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&hide]).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

fn clamp_window_to_screen(win: &tauri::WebviewWindow) {
    let Ok(pos) = win.outer_position() else {
        return;
    };
    let Ok(size) = win.outer_size() else {
        return;
    };
    let Ok(monitors) = win.available_monitors() else {
        return;
    };
    if monitors.is_empty() {
        return;
    }

    let w = size.width as i32;
    let h = size.height as i32;

    // Union bounding rect of all monitors
    let mut union_left = i32::MAX;
    let mut union_top = i32::MAX;
    let mut union_right = i32::MIN;
    let mut union_bottom = i32::MIN;
    for m in &monitors {
        let mp = m.position();
        let ms = m.size();
        union_left = union_left.min(mp.x);
        union_top = union_top.min(mp.y);
        union_right = union_right.max(mp.x + ms.width as i32);
        union_bottom = union_bottom.max(mp.y + ms.height as i32);
    }

    let max_x = (union_right - w).max(union_left);
    let max_y = (union_bottom - h).max(union_top);
    let x = pos.x.clamp(union_left, max_x);
    let y = pos.y.clamp(union_top, max_y);

    if x == pos.x && y == pos.y {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("AIPet")
                .build(),
        )
        .setup(|app| {
            tray::create_tray(app)?;
            let cfg = config_manager::read_or_create_app_config(app.handle())?;
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_shadow(false);
                let w = main_win.clone();
                main_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Moved { .. } = event {
                        clamp_window_to_screen(&w);
                    }
                });
            }
            config_manager::apply_runtime_from_config(app.handle(), &cfg);
            system_monitor::spawn_system_monitor(app.handle().clone());

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
        .invoke_handler(tauri::generate_handler![
            show_pet_menu,
            config_manager::get_app_config,
            config_manager::save_app_config,
            config_manager::get_state_config,
            config_manager::save_state_config,
            config_manager::list_pets,
            config_manager::get_pet_detail,
            config_manager::get_pet_spritesheet_path,
            config_manager::refresh_pets_dir,
            config_manager::get_app_data_path,
            config_manager::open_app_data_dir,
        ])
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
