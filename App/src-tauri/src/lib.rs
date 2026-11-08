pub mod action;
pub mod bt;
pub mod config;
pub mod device;
pub mod event;
pub mod platform;

use config::{Config, Device};
use event::Event;
use tauri_plugin_autostart::ManagerExt;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Emitter, Manager, State, WebviewWindowBuilder, WindowEvent,
};

struct AppState {
    config: Mutex<Config>,
    path: PathBuf,
    backend: platform::Handle,
    bt: bt::BtHandle,
    /// The latest battery / device / active-link values, for a page that loads after they were sent.
    status: Arc<Mutex<Status>>,
}

/// What a page that opens late has missed: these events fire at startup, often before the page listens.
#[derive(serde::Serialize, Clone, Default)]
struct Status {
    /// Battery % over Bluetooth; `None` = unknown ("—").
    battery: Option<u8>,
    /// Receiver plugged in / Bluetooth mouse connected, from the input backend; `None` = not reported yet.
    usb: Option<bool>,
    bluetooth: Option<bool>,
    /// The link the N230 last sent input on.
    active: Option<device::Bus>,
}

impl Status {
    fn note(&mut self, ev: &Event) {
        match ev {
            Event::Battery { percent } => self.battery = *percent,
            Event::Device { bus: Some(device::Bus::Usb), connected, .. } => self.usb = Some(*connected),
            Event::Device { bus: Some(device::Bus::Bluetooth), connected, .. } => self.bluetooth = Some(*connected),
            Event::Active { bus } => self.active = Some(*bus),
            _ => {}
        }
    }
}

/// The latest battery, device and active-link status (see `Status`).
#[tauri::command]
fn get_status(state: State<AppState>) -> Status {
    state.status.lock().unwrap().clone()
}

/// The Bluetooth addresses to watch for link state (B5): configured Bluetooth devices paired from the app.
fn bt_addresses(c: &Config) -> Vec<String> {
    c.devices.iter().filter(|d| d.bus == Some(device::Bus::Bluetooth)).filter_map(|d| d.address.clone()).collect()
}

/// Bluetooth adapter: "on", "off" or "missing".
#[tauri::command]
fn bt_adapter(state: State<AppState>) -> bt::AdapterState {
    state.bt.adapter()
}

/// Start or stop searching. Results arrive as `btFound` events; `btScan` says when it starts and ends.
#[tauri::command]
fn bt_scan(on: bool, state: State<AppState>) -> Result<(), String> {
    state.bt.scan(on)
}

/// Pairs the mouse at `address` (B4), then records its address and name on the config entry for its
/// VID/PID so it's remapped. Runs off the UI thread: pairing can take up to ~40 s.
/// Returns the device list (the Settings screen re-renders it) and a status line.
#[tauri::command]
async fn bt_pair(address: String, state: State<'_, AppState>) -> Result<(Vec<Device>, String), String> {
    let bt = state.bt.clone();
    let (paired, still_paired) = tauri::async_runtime::spawn_blocking(move || {
        let p = bt.pair(&address)?;
        let others: Vec<String> = bt.paired().into_iter().map(|m| m.address).collect(); // the other channel may be paired too
        Ok::<_, String>((p, others))
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut c = state.config.lock().unwrap();
    let mut next = c.clone();
    let message = if next.remember_bluetooth(paired.id, &paired.address, &paired.name, &still_paired) {
        state.backend.apply(next.clone())?;
        *c = next;
        c.save(&state.path).map_err(|e| format!("Paired, but couldn't save the config: {e}"))?;
        "Paired. Move the mouse to connect."
    } else if paired.id.is_none() {
        "Paired, but its VID/PID couldn't be read. If the side buttons aren't remapped, add it under Devices."
    } else {
        "Paired. Move the mouse to connect."
    };
    state.bt.watch(&bt_addresses(&c));
    Ok((c.devices.clone(), message.into()))
}

/// Unpairs the mouse in the OS and drops its address from the config (the VID/PID entry stays).
#[tauri::command]
async fn bt_forget(address: String, state: State<'_, AppState>) -> Result<Vec<Device>, String> {
    let bt = state.bt.clone();
    let addr = address.clone();
    tauri::async_runtime::spawn_blocking(move || bt.forget(&addr)).await.map_err(|e| e.to_string())??;
    let mut c = state.config.lock().unwrap();
    if c.forget_bluetooth(&address) {
        c.save(&state.path).map_err(|e| format!("Forgotten, but couldn't save the config: {e}"))?;
    }
    state.bt.watch(&bt_addresses(&c));
    Ok(c.devices.clone())
}

/// Mice the OS has paired over Bluetooth LE.
#[tauri::command]
async fn bt_paired(state: State<'_, AppState>) -> Result<Vec<bt::PairedMouse>, String> {
    let bt = state.bt.clone();
    tauri::async_runtime::spawn_blocking(move || bt.paired()).await.map_err(|e| e.to_string())
}

/// Opens the OS Bluetooth settings (`page` "bluetooth": switch the radio on, which the app never does
/// itself) or the radio privacy settings (`page` "radios": when Windows denies this app the radio).
#[tauri::command]
fn bt_open_settings(page: String, state: State<AppState>) -> Result<(), String> {
    state.bt.open_settings(&page)
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

/// Sets one side button's mapping. Bad text returns the parser's error (`X2: unknown key 'Nope'`)
/// and changes nothing; good text is live at once, then saved.
#[tauri::command]
fn set_mapping(button: String, mapping: String, state: State<AppState>) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    let mut next = config.clone();
    match button.as_str() {
        "X1" => next.buttons.x1 = mapping,
        "X2" => next.buttons.x2 = mapping,
        _ => return Err(format!("unknown button '{button}'")),
    }
    state.backend.apply(next.clone())?;
    *config = next;
    config.save(&state.path).map_err(|e| format!("Applied, but couldn't save the config: {e}"))
}

/// Saves the measured DPI and the chosen stage (`None` = Native) and applies target ÷ measured.
#[tauri::command]
fn set_dpi(measured: Option<f64>, target: Option<u32>, state: State<AppState>) -> Result<(), String> {
    if measured.is_some_and(|m| !(m.is_finite() && m > 0.0)) {
        return Err("measured DPI must be a positive number".into());
    }
    let mut config = state.config.lock().unwrap();
    config.measured_dpi = measured;
    config.target_dpi = target;
    state.backend.set_scale(config.scale());
    config.save(&state.path).map_err(|e| format!("Applied, but couldn't save the config: {e}"))
}

/// `--log <seconds>`: print what every mouse sends, tagged by device, like the C# app's `--log`.
/// Uses the default device list and remaps nothing. Debug builds only have a console.
pub fn log(seconds: u64) {
    let (tx, rx) = mpsc::channel();
    let backend = platform::start(Config { buttons: Default::default(), ..Config::default() }, tx);
    backend.set_all_devices(true);
    println!("Logging for {seconds}s - press each button a few times, slowly.");
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let Ok(ev) = rx.recv_timeout(left) else { break };
        let t = start.elapsed().as_secs_f64();
        match ev {
            Event::Button { device, target, button, down, .. } => {
                let tag = if target { "N230 " } else { "other" };
                println!("{t:8.3} [{tag}] {button}-{}  {device}", if down { "down" } else { "up" });
            }
            Event::Move { target: true, dx, dy } => println!("{t:8.3} [N230 ] move {dx:+} {dy:+}"),
            Event::Move { .. } => {}
            other => println!("{t:8.3} {other:?}"),
        }
    }
}

/// `--scan [stop_after_secs]`: one Bluetooth search (B3), printing every device it reports. With a number,
/// it's stopped by hand after that long (the Stop button's path); without, it runs until the 30 s auto-stop.
/// Like `--log`, debug builds only have a console.
pub fn scan(stop_after: Option<u64>) {
    let (tx, rx) = mpsc::channel();
    let bt = bt::start(tx);
    println!("Bluetooth adapter: {:?}", bt.adapter());
    if let Err(e) = bt.scan(true) {
        println!("{e}");
        return;
    }
    let start = Instant::now();
    if let Some(secs) = stop_after {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(secs));
            println!("{secs:7}.000 stopping by hand");
            let _ = bt.scan(false);
        });
    }
    for ev in rx {
        println!("{:7.3} {ev:?}", start.elapsed().as_secs_f64());
        if ev == (Event::BtScan { active: false }) {
            break;
        }
    }
}

/// `--watch <address> [seconds]`: print link-state and adapter changes for one paired mouse (B5).
pub fn watch(address: String, seconds: u64) {
    let (tx, rx) = mpsc::channel();
    let bt = bt::start(tx);
    bt.watch(&[address]);
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let Ok(ev) = rx.recv_timeout(left) else { break };
        println!("{:8.3} {ev:?}", start.elapsed().as_secs_f64());
    }
}

/// `--paired`: the Bluetooth mice the OS has paired (B4), one per line.
pub fn paired() {
    let (tx, _rx) = mpsc::channel();
    for m in bt::start(tx).paired() {
        println!("{m:?}");
    }
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

/// Monitor: report events from other devices too (default: only configured ones).
#[tauri::command]
fn set_all_devices(on: bool, state: State<AppState>) {
    state.backend.set_all_devices(on);
}

/// Settings: replace the remapped device list. Validates before anything changes.
#[tauri::command]
fn set_devices(devices: Vec<Device>, state: State<AppState>) -> Result<(), String> {
    for d in &devices {
        if !config::valid_id(&d.vid) || !config::valid_id(&d.pid) {
            return Err(format!("device {}:{} needs a 4-digit hex VID and PID", d.vid, d.pid));
        }
    }
    let mut c = state.config.lock().unwrap();
    let mut next = c.clone();
    next.devices = devices;
    state.backend.apply(next.clone())?;
    *c = next;
    state.bt.watch(&bt_addresses(&c));
    c.save(&state.path).map_err(|e| format!("Applied, but couldn't save the config: {e}"))
}

#[tauri::command]
fn set_start_minimized(enabled: bool, state: State<AppState>) -> Result<(), String> {
    let mut c = state.config.lock().unwrap();
    c.start_minimized = enabled;
    c.save(&state.path)
}

#[tauri::command]
fn set_low_battery_warning(enabled: bool, state: State<AppState>) -> Result<(), String> {
    let mut c = state.config.lock().unwrap();
    c.low_battery_warning = enabled;
    c.save(&state.path)
}

/// "Start with Windows" lives in the autostart plugin, not `config.json`.
#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_autostart(enabled: bool, app: AppHandle) -> Result<(), String> {
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
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

/// One OS notification when the battery drops below 15%, if "Low battery warning" is on (Settings).
fn low_battery_notice(app: &AppHandle, percent: u8) {
    use tauri_plugin_notification::NotificationExt;
    let enabled = app.try_state::<AppState>().is_some_and(|s| s.config.lock().unwrap().low_battery_warning);
    if enabled {
        let _ = app
            .notification()
            .builder()
            .title("N230 battery low")
            .body(format!("{percent}% left. Charge it with the USB-C cable."))
            .show();
    }
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
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            set_theme,
            get_config,
            set_mapping,
            set_dpi,
            set_all_devices,
            set_devices,
            set_start_minimized,
            set_low_battery_warning,
            get_autostart,
            set_autostart,
            bt_adapter,
            bt_scan,
            bt_open_settings,
            bt_pair,
            bt_forget,
            bt_paired,
            get_status
        ])
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("config.json");
            let config = Config::load(&path);

            // The window has "create": false in tauri.conf.json so the theme can go in first.
            let (r, g, b) = config::theme_bg(&config.theme);
            WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?
                .background_color(Color(r, g, b, 255))
                .initialization_script(&theme_script(&config.theme))
                .visible(!config.start_minimized)
                .build()?;

            // Backend events go to the UI on one channel, "n230", as {"type": …, …}.
            let (tx, rx) = mpsc::channel();
            let bt = bt::start(tx.clone());
            bt.watch(&bt_addresses(&config));
            let backend = platform::start(config.clone(), tx);
            backend.set_scale(config.scale());
            let handle = app.handle().clone();
            let status = Arc::new(Mutex::new(Status::default()));
            let last = status.clone();
            thread::spawn(move || {
                let mut low = bt::LowBattery::default();
                for ev in rx {
                    last.lock().unwrap().note(&ev);
                    if let Event::Battery { percent: Some(p) } = ev {
                        if low.check(p) {
                            low_battery_notice(&handle, p);
                        }
                    }
                    let _ = handle.emit("n230", ev);
                }
            });
            app.manage(AppState { config: Mutex::new(config), path, backend, bt, status });

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

#[cfg(test)]
mod tests {
    use super::*;
    use device::Bus;

    #[test]
    fn status_keeps_the_latest_values() {
        let mut s = Status::default();
        s.note(&Event::Battery { percent: Some(83) });
        s.note(&Event::Device { bus: Some(Bus::Usb), connected: true, permission: true });
        s.note(&Event::Device { bus: Some(Bus::Bluetooth), connected: true, permission: true });
        s.note(&Event::Active { bus: Bus::Bluetooth });
        s.note(&Event::Device { bus: None, connected: false, permission: true }); // unknown bus: ignored
        s.note(&Event::Error { message: "x".into() });
        assert_eq!((s.battery, s.usb, s.bluetooth, s.active), (Some(83), Some(true), Some(true), Some(Bus::Bluetooth)));
        s.note(&Event::Battery { percent: None });
        s.note(&Event::Device { bus: Some(Bus::Bluetooth), connected: false, permission: true });
        assert_eq!((s.battery, s.bluetooth), (None, Some(false)), "asleep: no stale battery");
    }
}
