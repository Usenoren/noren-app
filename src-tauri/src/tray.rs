use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter,
};

use crate::window;

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let profiles = MenuItem::with_id(app, "profiles", "Profiles", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Noren", true, Some("CmdOrCtrl+Q"))?;

    let menu = MenuBuilder::new(app)
        .item(&open)
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
                window::show_popup(app);
            }
            "settings" => {
                window::show_popup(app);
                let _ = app.emit("navigate", "settings");
            }
            "profiles" => {
                window::show_popup(app);
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
                window::toggle_popup(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
