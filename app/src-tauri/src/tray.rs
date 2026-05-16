use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

fn create_placeholder_icon() -> tauri::image::Image<'static> {
    let size = 32u32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|_| [236u8, 72, 153, 255])
        .collect();
    tauri::image::Image::new_owned(rgba, size, size)
}

pub fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_pet_i = MenuItem::with_id(app, "show_pet", "显示宠物", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "显示设置", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_pet_i, &settings_i, &quit_i])?;

    let icon = create_placeholder_icon();

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("AIPet")
        .menu(&menu)
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show_pet" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
