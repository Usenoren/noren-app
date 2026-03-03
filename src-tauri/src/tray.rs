use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter,
};

use crate::window;

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let open = MenuItem::with_id(app, "open", "Open Noren", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Quick Access", true, Some("CmdOrCtrl+K"))?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let profiles = MenuItem::with_id(app, "profiles", "Profiles", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Noren", true, Some("CmdOrCtrl+Q"))?;

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&quick)
        .separator()
        .item(&profiles)
        .item(&settings)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Noren")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => {
                app.exit(0);
            }
            "open" => {
                window::show_main_window(app);
            }
            "quick" => {
                window::toggle_popup(app);
            }
            "settings" => {
                window::show_main_window(app);
                let _ = app.emit("navigate", "settings");
            }
            "profiles" => {
                window::show_main_window(app);
                let _ = app.emit("navigate", "profiles");
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
