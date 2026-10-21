//! Windows backend, a port of `Monitor/Remapper.cs` (see the wiki's Windows Input Backend page).
//!
//! One thread owns a message-only window, Raw Input and a `WH_MOUSE_LL` hook.
//! - Raw Input knows which device sent a click but can't block it; the hook can block it but
//!   doesn't know the device. On X1/X2 down, the hook drains the queued `WM_INPUT` to learn the
//!   device, decides, and keeps that decision for the matching up.
//! - The hook only decides and queues. Actions run on `WM_APP`, outside the hook.
//! - Re-entrancy: low-level hook calls arrive as sent messages, so `PeekMessage` can call the hook
//!   again. The state is borrowed only briefly and never across `PeekMessage` or `SendInput`.

use crate::{
    action::{self, Action, Btn, Key},
    config::{Config, Device},
    event::Event,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
    System::{
        Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE},
        LibraryLoader::GetModuleHandleW,
    },
    UI::{
        Input::{
            GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT, KeyboardAndMouse::*,
            RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RIDI_DEVICENAME, RID_INPUT, RIM_TYPEMOUSE,
        },
        Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};

const WM_WAKE: u32 = WM_APP;
const MOVE_TIMER_MS: u32 = 50;
/// Hook liveness is checked once per this many move-timer ticks (1 s).
const LIVENESS_TICKS: u32 = 20;

enum Cmd {
    Apply([Action; 2], Vec<Device>),
    Scale(f64),
    AllDevices(bool),
}

enum Job {
    Act(Action, bool),
    FlushScaledMove,
}

pub struct Handle {
    cmds: Sender<Cmd>,
    hwnd: isize,
}

pub fn start(config: Config, events: Sender<Event>) -> Handle {
    let (cmds, cmd_rx) = mpsc::channel();
    let (hwnd_tx, hwnd_rx) = mpsc::sync_channel(1);
    let thread_events = events.clone();
    let spawned = thread::Builder::new().name("n230-input".into()).spawn(move || unsafe {
        match setup(thread_events.clone(), cmd_rx) {
            Ok(hwnd) => {
                let _ = hwnd_tx.send(hwnd as isize);
                message_loop();
            }
            Err(message) => {
                let _ = thread_events.send(Event::Error { message });
                let _ = hwnd_tx.send(0);
            }
        }
    });
    let hwnd = if spawned.is_ok() { hwnd_rx.recv().unwrap_or(0) } else { 0 };
    let handle = Handle { cmds, hwnd };
    if let Err(message) = handle.apply(config) {
        let _ = events.send(Event::Error { message });
    }
    handle
}

impl Handle {
    pub fn apply(&self, config: Config) -> Result<(), String> {
        let (x1, x2) = action::parse_buttons(&config.buttons)?;
        self.send(Cmd::Apply([x1, x2], config.devices));
        Ok(())
    }

    pub fn set_scale(&self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.send(Cmd::Scale(factor));
        }
    }

    pub fn set_all_devices(&self, on: bool) {
        self.send(Cmd::AllDevices(on));
    }

    fn send(&self, cmd: Cmd) {
        if self.cmds.send(cmd).is_ok() {
            unsafe { PostMessageW(self.hwnd as HWND, WM_WAKE, 0, 0) };
        }
    }
}

struct State {
    hwnd: HWND,
    hook: HHOOK,
    events: Sender<Event>,
    cmds: Receiver<Cmd>,
    /// X1, X2.
    actions: [Action; 2],
    /// The decision made at button-down, reused at button-up. `None` = pass through.
    held: [Option<Action>; 2],
    devices: Vec<Device>,
    /// Raw device handle → (path, is a configured device).
    names: HashMap<isize, (String, bool)>,
    all_devices: bool,
    click_from_target: bool,
    move_from_target: bool,
    jobs: VecDeque<Job>,
    scale: f64,
    scaled: (i64, i64),
    carry: (f64, f64),
    /// Movement for the UI since the last timer tick: [other devices, target].
    ui_move: [(i32, i32); 2],
    raw_count: u64,
    hook_count: u64,
    seen: (u64, u64),
    ticks: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Runs `f` on the state. `None` if there is no state or it's already borrowed (re-entered).
fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn setup(events: Sender<Event>, cmds: Receiver<Cmd>) -> Result<HWND, String> {
    unsafe {
        let inst = GetModuleHandleW(null());
        let class = wide("N230ControlInput");
        let mut wc: WNDCLASSW = zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = inst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(0, class.as_ptr(), null(), 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), inst, null());
        if hwnd.is_null() {
            return Err(format!("couldn't create the input window (error {})", GetLastError()));
        }
        // The only Raw Input registration in the process (one window per usage page per process).
        let rid = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
        if RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            return Err(format!("couldn't register for raw mouse input (error {})", GetLastError()));
        }
        let hook = install_hook()?;
        SetTimer(hwnd, 1, MOVE_TIMER_MS, None);
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                hwnd,
                hook,
                events,
                cmds,
                actions: [Action::PassThrough, Action::PassThrough],
                held: [None, None],
                devices: Vec::new(),
                names: HashMap::new(),
                all_devices: false,
                click_from_target: false,
                move_from_target: false,
                jobs: VecDeque::new(),
                scale: 1.0,
                scaled: (0, 0),
                carry: (0.0, 0.0),
                ui_move: [(0, 0); 2],
                raw_count: 0,
                hook_count: 0,
                seen: (0, 0),
                ticks: 0,
            })
        });
        Ok(hwnd)
    }
}

unsafe fn install_hook() -> Result<HHOOK, String> {
    unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
        if hook.is_null() {
            Err(format!("couldn't install the mouse hook (error {})", GetLastError()))
        } else {
            Ok(hook)
        }
    }
}

unsafe fn message_loop() {
    unsafe {
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_INPUT => on_raw_input(lparam), // then DefWindowProc, which frees the raw input
            WM_WAKE => {
                on_wake();
                return 0;
            }
            WM_TIMER => {
                on_timer();
                return 0;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// No actions, no I/O, no UI in here: decide, queue, return. Never panics (that would abort).
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 {
            let info = &*(lparam as *const MSLLHOOKSTRUCT);
            let msg = wparam as u32;
            // Injected input (including our own SendInput) is never touched.
            if info.flags & LLMHF_INJECTED == 0 && hook_decides(msg, info.mouseData) {
                return 1;
            }
        }
        CallNextHookEx(null_mut(), code, wparam, lparam)
    }
}

/// True = swallow the original event.
unsafe fn hook_decides(msg: u32, mouse_data: u32) -> bool {
    unsafe {
        with(|s| s.hook_count += 1);
        match msg {
            WM_MOUSEMOVE if with(|s| s.scale != 1.0) == Some(true) => {
                drain_raw_input(); // learn which device this move came from
                with(|s| s.move_from_target).unwrap_or(false)
            }
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let i = if (mouse_data >> 16) as u16 == XBUTTON1 { 0 } else { 1 };
                let down = msg == WM_XBUTTONDOWN;
                if down {
                    drain_raw_input(); // this click's WM_INPUT is already queued
                    with(|s| {
                        let a = &s.actions[i];
                        s.held[i] = (s.click_from_target && *a != Action::PassThrough).then(|| a.clone());
                    });
                }
                // The up gets exactly the down's decision.
                let job = with(|s| if down { s.held[i].clone() } else { s.held[i].take() }).flatten();
                match job {
                    Some(a) => {
                        queue(Job::Act(a, down));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }
}

fn queue(job: Job) {
    if let Some(hwnd) = with(|s| {
        s.jobs.push_back(job);
        s.hwnd
    }) {
        unsafe { PostMessageW(hwnd, WM_WAKE, 0, 0) };
    }
}

unsafe fn drain_raw_input() {
    unsafe {
        let Some(hwnd) = with(|s| s.hwnd) else { return };
        let mut msg: MSG = zeroed();
        while PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) != 0 {
            on_raw_input(msg.lParam);
            DefWindowProcW(hwnd, WM_INPUT, msg.wParam, msg.lParam);
        }
    }
}

const RI_BUTTON_MASK: u16 = 0x3FF;
const MOUSE_MOVE_ABSOLUTE: u16 = 1;

/// The button transitions in a raw mouse event's `usButtonFlags`, as ("X1", down).
pub fn button_changes(flags: u16) -> impl Iterator<Item = (&'static str, bool)> {
    const NAMES: [&str; 5] = ["Left", "Right", "Middle", "X1", "X2"];
    (0..10).filter(move |bit| flags & (1 << bit) != 0).map(|bit| (NAMES[bit / 2], bit % 2 == 0))
}

unsafe fn on_raw_input(lparam: LPARAM) {
    unsafe {
        let mut raw: RAWINPUT = zeroed();
        let mut size = size_of::<RAWINPUT>() as u32;
        let header = size_of::<RAWINPUTHEADER>() as u32;
        let read = GetRawInputData(lparam as HRAWINPUT, RID_INPUT, &mut raw as *mut _ as *mut c_void, &mut size, header);
        // Injected input has no device handle.
        if read == u32::MAX || raw.header.dwType != RIM_TYPEMOUSE || raw.header.hDevice.is_null() {
            return;
        }
        let m = raw.data.mouse;
        let flags = m.Anonymous.Anonymous.usButtonFlags;
        let (dx, dy) = if m.usFlags & MOUSE_MOVE_ABSOLUTE == 0 { (m.lLastX, m.lLastY) } else { (0, 0) };
        if dx == 0 && dy == 0 && flags == 0 {
            return;
        }
        let device = raw.header.hDevice as isize;
        let flush = with(|s| {
            s.raw_count += 1;
            let (path, target) = s
                .names
                .entry(device)
                .or_insert_with(|| {
                    let path = device_name(device);
                    let target = matches_device(&path, &s.devices);
                    (path, target)
                })
                .clone();
            let mut flush = false;
            if dx != 0 || dy != 0 {
                s.move_from_target = target;
                if target || s.all_devices {
                    let m = &mut s.ui_move[target as usize];
                    *m = (m.0.saturating_add(dx), m.1.saturating_add(dy));
                }
                if target && s.scale != 1.0 {
                    flush = s.scaled == (0, 0);
                    s.scaled = (s.scaled.0 + dx as i64, s.scaled.1 + dy as i64);
                }
            }
            if flags & RI_BUTTON_MASK != 0 {
                s.click_from_target = target;
                if target || s.all_devices {
                    for (button, down) in button_changes(flags) {
                        let _ = s.events.send(Event::Button { device: path.clone(), target, button: button.into(), down });
                    }
                }
            }
            flush
        });
        if flush == Some(true) {
            queue(Job::FlushScaledMove);
        }
    }
}

unsafe fn device_name(device: isize) -> String {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(device as _, RIDI_DEVICENAME, null_mut(), &mut len);
        let mut buf = vec![0u16; len as usize + 1];
        if GetRawInputDeviceInfoW(device as _, RIDI_DEVICENAME, buf.as_mut_ptr() as *mut c_void, &mut len) == u32::MAX {
            return String::new();
        }
        String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string()
    }
}

/// True if a Raw Input device path belongs to one of the configured devices.
/// USB: `…VID_3554&PID_FC01…`. Bluetooth: `…_VID&0002046d_PID&b01a…` or `…_VID&02046d_PID&b023…`.
pub fn matches_device(path: &str, devices: &[Device]) -> bool {
    let p = path.to_ascii_uppercase();
    devices.iter().any(|d| {
        let (vid, pid) = (d.vid.to_ascii_uppercase(), d.pid.to_ascii_uppercase());
        let hex4 = |s: &str| s.len() == 4 && s.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex4(&vid) || !hex4(&pid) {
            return false; // a blank or bad entry must never match every mouse
        }
        p.contains(&format!("VID_{vid}&PID_{pid}"))
            || p.match_indices("VID&").any(|(i, _)| {
                let rest = &p[i + 4..];
                let end = rest.find('_').unwrap_or(rest.len());
                rest[..end].ends_with(&vid) && rest[end..].starts_with(&format!("_PID&{pid}"))
            })
    })
}

unsafe fn on_wake() {
    unsafe {
        with(|s| {
            while let Ok(cmd) = s.cmds.try_recv() {
                match cmd {
                    Cmd::Apply(actions, devices) => {
                        s.actions = actions;
                        s.devices = devices;
                        s.names.clear(); // target flags depend on the device list
                    }
                    Cmd::Scale(f) => {
                        s.scale = f;
                        s.scaled = (0, 0);
                        s.carry = (0.0, 0.0);
                    }
                    Cmd::AllDevices(on) => s.all_devices = on,
                }
            }
        });
        while let Some(job) = with(|s| s.jobs.pop_front()).flatten() {
            match job {
                Job::Act(a, down) => run_action(a, down),
                Job::FlushScaledMove => flush_scaled_move(),
            }
        }
    }
}

unsafe fn flush_scaled_move() {
    unsafe {
        let mv = with(|s| {
            let (x, y) = (s.scaled.0 as f64 * s.scale + s.carry.0, s.scaled.1 as f64 * s.scale + s.carry.1);
            let (ix, iy) = (x.trunc(), y.trunc());
            s.carry = (x - ix, y - iy); // carry the fraction so slow moves aren't lost
            s.scaled = (0, 0);
            (ix as i32, iy as i32)
        });
        if let Some((dx, dy)) = mv.filter(|m| *m != (0, 0)) {
            send(&[mouse_input(dx, dy, 0, MOUSEEVENTF_MOVE)]);
        }
    }
}

unsafe fn run_action(a: Action, down: bool) {
    unsafe {
        match a {
            Action::PassThrough | Action::Disabled => {}
            Action::Mouse(b) => {
                let (d, u, data) = match b {
                    Btn::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
                    Btn::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
                    Btn::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
                    Btn::Back => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON1 as u32),
                    Btn::Forward => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON2 as u32),
                };
                send(&[mouse_input(0, 0, data, if down { d } else { u })]);
            }
            // Press in order on down, release in reverse on up, so holding works.
            Action::Keys(keys) => {
                let inputs: Vec<INPUT> = if down {
                    keys.iter().map(|k| key_input(*k, true)).collect()
                } else {
                    keys.iter().rev().map(|k| key_input(*k, false)).collect()
                };
                send(&inputs);
            }
            Action::Run(target) if down => {
                // On its own thread: a slow launch must not stall the hook's thread.
                let events = with(|s| s.events.clone());
                thread::spawn(move || {
                    CoInitializeEx(null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
                    let r = ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(&target).as_ptr(), null(), null(), SW_SHOWNORMAL);
                    if (r as isize) <= 32 {
                        if let Some(ev) = events {
                            let _ = ev.send(Event::Error { message: format!("run: couldn't open '{target}'") });
                        }
                    }
                });
            }
            Action::Run(_) => {}
        }
    }
}

unsafe fn on_timer() {
    unsafe {
        let lost = with(|s| {
            for (i, target) in [(0, false), (1, true)] {
                let (dx, dy) = std::mem::take(&mut s.ui_move[i]);
                if (dx, dy) != (0, 0) {
                    let _ = s.events.send(Event::Move { target, dx, dy });
                }
            }
            s.ticks += 1;
            if s.ticks % LIVENESS_TICKS != 0 {
                return false;
            }
            let (raw, hook) = (s.raw_count - s.seen.0, s.hook_count - s.seen.1);
            s.seen = (s.raw_count, s.hook_count);
            hook_lost(raw, hook)
        });
        if lost == Some(true) {
            // Windows silently removes a hook that once took too long. Put it back.
            let old = with(|s| s.hook).unwrap_or(null_mut());
            UnhookWindowsHookEx(old);
            let message = match install_hook() {
                Ok(hook) => {
                    with(|s| s.hook = hook);
                    "The mouse hook was removed by Windows and has been re-installed.".to_string()
                }
                Err(e) => e,
            };
            with(|s| s.events.send(Event::Error { message }));
        }
    }
}

/// Real mouse input reached Raw Input but none reached the hook: the hook is gone.
fn hook_lost(raw_events: u64, hook_events: u64) -> bool {
    raw_events >= 10 && hook_events == 0
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

unsafe fn key_input(key: Key, down: bool) -> INPUT {
    unsafe {
        let (vk, extended) = vk(key);
        let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        let scan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16;
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        }
    }
}

unsafe fn send(inputs: &[INPUT]) {
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
}

/// Windows virtual-key code and whether it's an extended key. Exhaustive on purpose.
pub fn vk(key: Key) -> (u16, bool) {
    use Key::*;
    match key {
        Ctrl => (0x11, false),
        Shift => (0x10, false),
        Alt => (0x12, false),
        Win => (0x5B, true),
        Letter(c) => (c as u16, false),
        Digit(d) => (0x30 + d as u16, false),
        F(n) => (0x70 + n as u16 - 1, false),
        NumPad(n) => (0x60 + n as u16, false),
        Enter => (0x0D, false),
        Escape => (0x1B, false),
        Tab => (0x09, false),
        Space => (0x20, false),
        Backspace => (0x08, false),
        Delete => (0x2E, true),
        Insert => (0x2D, true),
        Home => (0x24, true),
        End => (0x23, true),
        PageUp => (0x21, true),
        PageDown => (0x22, true),
        Left => (0x25, true),
        Up => (0x26, true),
        Right => (0x27, true),
        Down => (0x28, true),
        CapsLock => (0x14, false),
        NumLock => (0x90, true),
        ScrollLock => (0x91, false),
        PrintScreen => (0x2C, true),
        Pause => (0x13, false),
        Apps => (0x5D, true),
        Multiply => (0x6A, false),
        Add => (0x6B, false),
        Subtract => (0x6D, false),
        Decimal => (0x6E, false),
        Divide => (0x6F, true),
        Minus => (0xBD, false),
        Equals => (0xBB, false),
        Comma => (0xBC, false),
        Period => (0xBE, false),
        Slash => (0xBF, false),
        Grave => (0xC0, false),
        LeftBracket => (0xDB, false),
        Backslash => (0xDC, false),
        RightBracket => (0xDD, false),
        Semicolon => (0xBA, false),
        Quote => (0xDE, false),
        VolumeMute => (0xAD, true),
        VolumeDown => (0xAE, true),
        VolumeUp => (0xAF, true),
        MediaNextTrack => (0xB0, true),
        MediaPreviousTrack => (0xB1, true),
        MediaStop => (0xB2, true),
        MediaPlayPause => (0xB3, true),
        BrowserBack => (0xA6, true),
        BrowserForward => (0xA7, true),
        BrowserRefresh => (0xA8, true),
        BrowserSearch => (0xAA, true),
        BrowserHome => (0xAC, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(vid: &str, pid: &str) -> Device {
        Device { vid: vid.into(), pid: pid.into() }
    }

    #[test]
    fn device_matching() {
        let n230 = [dev("3554", "fc01")];
        assert!(matches_device(r"\\?\HID#VID_3554&PID_FC01&MI_00&Col01#8&1d0b0c4a&0&0000#{378de44c}", &n230));
        assert!(!matches_device(r"\\?\HID#VID_046D&PID_C52B&MI_01&Col01#8&2#{378de44c}", &n230));
        assert!(!matches_device("", &n230), "injected input");
        let bt = [dev("046D", "B01A")];
        assert!(matches_device(r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002046d_PID&b01a&Col01#9&3", &bt));
        assert!(matches_device(r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b01a_REV&0007#a", &bt));
        assert!(!matches_device(r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002046d_PID&b01b&Col01#9&3", &bt));
        for bad in [dev("", ""), dev("35", "FC01"), dev("3554", "FC0"), dev("xyz1", "FC01")] {
            assert!(!matches_device(r"\\?\HID#VID_3554&PID_FC01&Col01", &[bad.clone()]), "{bad:?} must match nothing");
        }
    }

    // Ports of the raw-input checks in SelfTest() (Monitor/Program.cs).
    #[test]
    fn raw_button_flags() {
        assert_eq!(button_changes(0).count(), 0, "pure movement has no buttons");
        assert_eq!(button_changes(0x40).collect::<Vec<_>>(), [("X1", true)]);
        assert_eq!(button_changes(0x200).collect::<Vec<_>>(), [("X2", false)]);
        assert_eq!(button_changes(0x1 | 0x8).collect::<Vec<_>>(), [("Left", true), ("Right", false)]);
        assert_eq!(button_changes(0x400).count(), 0, "wheel is not a button");
    }

    #[test]
    fn input_struct_size() {
        assert_eq!(size_of::<INPUT>(), if size_of::<usize>() == 8 { 40 } else { 28 });
    }

    #[test]
    fn virtual_keys() {
        assert_eq!(vk(Key::Ctrl), (0x11, false));
        assert_eq!(vk(Key::Letter('T')), (0x54, false));
        assert_eq!(vk(Key::Digit(5)), (0x35, false));
        assert_eq!(vk(Key::F(1)), (0x70, false));
        assert_eq!(vk(Key::F(24)), (0x87, false));
        assert_eq!(vk(Key::VolumeUp), (0xAF, true));
    }

    #[test]
    fn hook_liveness() {
        assert!(hook_lost(25, 0));
        assert!(!hook_lost(25, 3), "the hook still sees input");
        assert!(!hook_lost(3, 0), "too little input to tell");
    }
}
