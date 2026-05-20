mod ai_api_client;
mod ai_config;
mod config_manager;
mod image_processor;
mod pet_creator;
mod prompt_builder;
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

#[tauri::command]
fn clamp_main_window_to_screen(app: tauri::AppHandle) -> Result<(), String> {
    let Some(main_win) = app.get_webview_window("main") else {
        return Err("main window not found".into());
    };
    clamp_window_to_screen(&main_win);
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

    // 对每个显示器计算"将窗口完全限制在该显示器内"所需的目标位置，
    // 选择距当前位置最近的那个——天然支持 T 形、L 形等不规则多屏布局。
    let mut best_x = pos.x;
    let mut best_y = pos.y;
    let mut best_dist = i64::MAX;

    for m in &monitors {
        let mp = m.position();
        let ms = m.size();
        let max_x = (mp.x + ms.width as i32 - w).max(mp.x);
        let max_y = (mp.y + ms.height as i32 - h).max(mp.y);
        let cx = pos.x.clamp(mp.x, max_x);
        let cy = pos.y.clamp(mp.y, max_y);
        let dx = (pos.x - cx) as i64;
        let dy = (pos.y - cy) as i64;
        let dist = dx * dx + dy * dy;
        if dist < best_dist {
            best_dist = dist;
            best_x = cx;
            best_y = cy;
        }
    }

    if best_x == pos.x && best_y == pos.y {
        return;
    }
    let _ = win.set_position(PhysicalPosition::new(best_x, best_y));
}

fn position_main_window_bottom_right(
    win: &tauri::WebviewWindow,
    cfg: &config_manager::AppConfig,
) {
    let Ok(Some(monitor)) = win.primary_monitor() else {
        return;
    };
    let mp = monitor.position();
    let ms = monitor.size();
    let sf = monitor.scale_factor();
    let win_w = (192.0 * cfg.animation_scale * sf).round() as i32;
    let win_h = (208.0 * cfg.animation_scale * sf).round() as i32;
    let margin_right = (20.0 * sf).round() as i32;
    let margin_bottom = (80.0 * sf).round() as i32;
    let x = mp.x + ms.width as i32 - win_w - margin_right;
    let y = mp.y + ms.height as i32 - win_h - margin_bottom;
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

#[tauri::command]
fn reset_main_window_position(app: tauri::AppHandle) -> Result<(), String> {
    let cfg = config_manager::read_or_create_app_config(&app)?;
    let Some(main_win) = app.get_webview_window("main") else {
        return Err("main window not found".into());
    };
    position_main_window_bottom_right(&main_win, &cfg);
    clamp_window_to_screen(&main_win);
    Ok(())
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            tray::create_tray(app)?;
            config_manager::seed_default_pets(app.handle())?;
            let cfg = config_manager::read_or_create_app_config(app.handle())?;

            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_shadow(false);
                position_main_window_bottom_right(&main_win, &cfg);
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
            clamp_main_window_to_screen,
            reset_main_window_position,
            config_manager::get_app_config,
            config_manager::save_app_config,
            config_manager::get_state_config,
            config_manager::save_state_config,
            config_manager::list_pets,
            config_manager::get_pet_detail,
            config_manager::get_pet_spritesheet_path,
            config_manager::delete_pet,
            config_manager::refresh_pets_dir,
            config_manager::get_app_data_path,
            config_manager::open_app_data_dir,
            system_monitor::list_running_processes,
            ai_config::get_ai_api_config,
            ai_config::save_ai_api_config,
            pet_creator::start_pet_creation,
            pet_creator::list_incomplete_tasks,
            pet_creator::resume_pet_creation,
            pet_creator::cancel_pet_creation,
            pet_creator::confirm_base_image,
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
