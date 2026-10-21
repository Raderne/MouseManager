pub mod action;
pub mod config;
pub mod event;
pub mod platform;

use config::Config;
use std::{path::PathBuf, sync::Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Manager, State, WebviewWindowBuilder, WindowEvent,
};

struct AppState {
    config: Mutex<Config>,
    path: PathBuf,
}

#[tauri::command]
fn set_theme(theme: String, state: State<AppState>) -> Result<(), String> {
    if !config::is_theme(&theme) {
        return Err(format!("unknown theme '{theme}'"));
    }
    let mut c = state.config.lock().unwrap();
    c.theme = theme;
    c.save(&state.path)
}

/// Sets `data-theme` on <html> before any page script or paint, so there's no flash.
fn theme_script(theme: &str) -> String {
    let t = serde_json::to_string(theme).unwrap();
    format!(
        "(function(){{var s=function(){{document.documentElement.dataset.theme={t}}};\
         if(document.documentElement)s();else new MutationObserver(function(_,o){{\
         if(document.documentElement){{s();o.disconnect()}}}}).observe(document,{{childList:true}})}})()"
    )
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch exits and focuses this one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![set_theme])
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("config.json");
            let config = Config::load(&path);

            // The window has "create": false in tauri.conf.json so the theme can go in first.
            let (r, g, b) = config::theme_bg(&config.theme);
            WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .background_color(Color(r, g, b, 255))
                .initialization_script(&theme_script(&config.theme))
                .build()?;
            app.manage(AppState { config: Mutex::new(config), path });

            let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("N230 Control")
                .menu(&Menu::with_items(app, &[&open, &exit])?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_main(app),
                    "exit" => app.exit(0),
                    _ => {}
                })
                // Left click opens the window (Windows; Linux trays only show the menu).
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        // Closing the window hides it to the tray; only tray > Exit quits.
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
