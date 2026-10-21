//! The mapping parser, a port of `Monitor/ButtonAction.cs`. The only place mapping text is parsed.
//! Syntax: `Default`, `Disabled`, a mouse button (`Back`), a key combo (`Ctrl+Shift+T`, `VolumeUp`)
//! or `run:<program, file or URL>`. Names ignore case.

use crate::config::Buttons;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave the click alone.
    PassThrough,
    /// Swallow the click.
    Disabled,
    Mouse(Btn),
    /// Pressed in order on button-down, released in reverse on button-up, so holding works.
    Keys(Vec<Key>),
    Run(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// OS-neutral keys. Each platform module maps every variant (Windows VK, Linux `KEY_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Ctrl, Shift, Alt, Win,
    /// `'A'..='Z'`
    Letter(char),
    /// `0..=9`, the top row.
    Digit(u8),
    /// `1..=24`
    F(u8),
    /// `0..=9`
    NumPad(u8),
    Enter, Escape, Tab, Space, Backspace, Delete, Insert, Home, End, PageUp, PageDown,
    Left, Right, Up, Down, CapsLock, NumLock, ScrollLock, PrintScreen, Pause, Apps,
    Multiply, Add, Subtract, Decimal, Divide,
    Minus, Equals, Comma, Period, Slash, Grave, LeftBracket, RightBracket, Backslash, Semicolon, Quote,
    VolumeUp, VolumeDown, VolumeMute, MediaPlayPause, MediaNextTrack, MediaPreviousTrack, MediaStop,
    BrowserBack, BrowserForward, BrowserRefresh, BrowserHome, BrowserSearch,
}

const MOUSE: [(&str, Btn); 5] = [
    ("left", Btn::Left),
    ("right", Btn::Right),
    ("middle", Btn::Middle),
    ("back", Btn::Back),
    ("forward", Btn::Forward),
];

/// Lower-case names and aliases. Includes the WinForms `Keys` names old configs may use.
/// "back" here is Backspace (as in `Keys.Back`); it only applies inside a combo,
/// because a whole mapping of "Back" is matched as the mouse button first.
const KEYS: &[(&str, Key)] = {
    use Key::*;
    &[
        ("ctrl", Ctrl), ("control", Ctrl), ("controlkey", Ctrl),
        ("shift", Shift), ("shiftkey", Shift),
        ("alt", Alt), ("menu", Alt),
        ("win", Win), ("lwin", Win),
        ("enter", Enter), ("return", Enter),
        ("escape", Escape), ("esc", Escape),
        ("tab", Tab), ("space", Space),
        ("backspace", Backspace), ("back", Backspace),
        ("delete", Delete), ("del", Delete),
        ("insert", Insert), ("home", Home), ("end", End),
        ("pageup", PageUp), ("prior", PageUp), ("pagedown", PageDown), ("next", PageDown),
        ("left", Left), ("right", Right), ("up", Up), ("down", Down),
        ("capslock", CapsLock), ("capital", CapsLock),
        ("numlock", NumLock), ("scrolllock", ScrollLock), ("scroll", ScrollLock),
        ("printscreen", PrintScreen), ("snapshot", PrintScreen),
        ("pause", Pause), ("apps", Apps),
        ("multiply", Multiply), ("add", Add), ("subtract", Subtract), ("decimal", Decimal), ("divide", Divide),
        ("minus", Minus), ("oemminus", Minus),
        ("equals", Equals), ("oemplus", Equals),
        ("comma", Comma), ("oemcomma", Comma),
        ("period", Period), ("oemperiod", Period),
        ("slash", Slash), ("oemquestion", Slash),
        ("grave", Grave), ("oemtilde", Grave),
        ("leftbracket", LeftBracket), ("oemopenbrackets", LeftBracket),
        ("rightbracket", RightBracket), ("oemclosebrackets", RightBracket),
        ("backslash", Backslash), ("oempipe", Backslash),
        ("semicolon", Semicolon), ("oemsemicolon", Semicolon),
        ("quote", Quote), ("oemquotes", Quote),
        ("volumeup", VolumeUp), ("volumedown", VolumeDown), ("volumemute", VolumeMute),
        ("mediaplaypause", MediaPlayPause), ("medianexttrack", MediaNextTrack),
        ("mediaprevioustrack", MediaPreviousTrack), ("mediastop", MediaStop),
        ("browserback", BrowserBack), ("browserforward", BrowserForward),
        ("browserrefresh", BrowserRefresh), ("browserhome", BrowserHome), ("browsersearch", BrowserSearch),
    ]
};

pub fn parse(text: &str) -> Result<Action, String> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    if lower.is_empty() || lower == "default" {
        return Ok(Action::PassThrough);
    }
    if lower == "disabled" {
        return Ok(Action::Disabled);
    }
    if lower.starts_with("run:") {
        let target = text[4..].trim();
        return if target.is_empty() {
            Err("run: needs a program, file or URL".into())
        } else {
            Ok(Action::Run(target.into()))
        };
    }
    // Mouse buttons before keys: "Back" is the mouse button, not Backspace.
    if let Some((_, b)) = MOUSE.iter().find(|(n, _)| *n == lower) {
        return Ok(Action::Mouse(*b));
    }
    text.split('+').map(|t| parse_key(t.trim())).collect::<Result<_, _>>().map(Action::Keys)
}

pub fn parse_key(token: &str) -> Result<Key, String> {
    let t = token.to_ascii_lowercase();
    if let Some((_, k)) = KEYS.iter().find(|(n, _)| *n == t) {
        return Ok(*k);
    }
    // Plain digits only, no leading zero ("F05" is rejected).
    let num = |s: &str, max: u8| {
        let plain = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
        s.parse::<u8>().ok().filter(|n| plain && *n <= max)
    };
    let key = match t.as_bytes() {
        [c] if c.is_ascii_lowercase() => Some(Key::Letter(c.to_ascii_uppercase() as char)),
        [c] if c.is_ascii_digit() => Some(Key::Digit(c - b'0')),
        _ => match t.strip_prefix('f') {
            Some(n) => num(n, 24).filter(|n| *n >= 1).map(Key::F),
            None => t.strip_prefix("numpad").and_then(|d| num(d, 9)).map(Key::NumPad),
        },
    };
    key.ok_or_else(|| format!("unknown key '{token}'"))
}

/// Parses both side buttons. The error names the button, e.g. `X2: unknown key 'Nope'`.
pub fn parse_buttons(b: &Buttons) -> Result<(Action, Action), String> {
    let x1 = parse(&b.x1).map_err(|e| format!("X1: {e}"))?;
    let x2 = parse(&b.x2).map_err(|e| format!("X2: {e}"))?;
    Ok((x1, x2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Key::*;

    fn keys(text: &str) -> Vec<Key> {
        match parse(text) {
            Ok(Action::Keys(k)) => k,
            other => panic!("{text}: {other:?}"),
        }
    }

    // Ports of the ButtonAction checks in SelfTest() (Monitor/Program.cs).

    #[test]
    fn default_passes_through() {
        for t in ["Default", "default", " ", ""] {
            assert_eq!(parse(t), Ok(Action::PassThrough), "{t:?}");
        }
    }

    #[test]
    fn disabled_and_mouse_buttons() {
        assert_eq!(parse("disabled"), Ok(Action::Disabled));
        assert_eq!(parse("back"), Ok(Action::Mouse(Btn::Back)), "mouse name before Backspace");
        assert_eq!(parse("Forward"), Ok(Action::Mouse(Btn::Forward)));
        assert_eq!(parse("MIDDLE"), Ok(Action::Mouse(Btn::Middle)));
    }

    #[test]
    fn modifiers() {
        assert_eq!(parse_key("ctrl"), Ok(Ctrl));
        assert_eq!(parse_key("Win"), Ok(Win));
        assert_eq!(parse_key("alt"), Ok(Alt));
    }

    #[test]
    fn digits_fkeys_aliases() {
        assert_eq!(parse_key("5"), Ok(Digit(5)));
        assert_eq!(parse_key("f5"), Ok(F(5)));
        assert_eq!(parse_key("F24"), Ok(F(24)));
        assert_eq!(parse_key("Backspace"), Ok(Backspace));
        assert_eq!(parse_key("Return"), Ok(Enter));
        assert_eq!(parse_key("NumPad7"), Ok(NumPad(7)));
        assert_eq!(parse_key("t"), Ok(Letter('T')));
    }

    #[test]
    fn combos_and_run() {
        assert_eq!(keys("Ctrl+Shift+T"), [Ctrl, Shift, Letter('T')]);
        assert_eq!(keys(" ctrl + shift + t "), [Ctrl, Shift, Letter('T')]);
        assert_eq!(keys("VolumeUp"), [VolumeUp]);
        assert_eq!(keys("Alt+Left"), [Alt, Left], "Left is a key inside a combo");
        assert_eq!(keys("Ctrl+Back"), [Ctrl, Backspace], "as in the C# app");
        assert_eq!(keys("F+5"), [Letter('F'), Digit(5)]);
        assert_eq!(parse("run:calc.exe"), Ok(Action::Run("calc.exe".into())));
        assert_eq!(parse("RUN: https://Example.com "), Ok(Action::Run("https://Example.com".into())));
    }

    #[test]
    fn rejects_bad_text() {
        for bad in ["Ctrl+Nope", "Ctrl+", "run:", "run:   ", "Ctrl,Shift", "Alt+65536", "F0", "F25", "F05", "NumPad10", "NumPad", "é", "Ctrl++"] {
            assert!(parse(bad).is_err(), "should reject {bad:?}");
        }
        assert_eq!(parse("Ctrl+Nope"), Err("unknown key 'Nope'".into()));
    }

    #[test]
    fn error_names_the_button() {
        let b = |x1: &str, x2: &str| Buttons { x1: x1.into(), x2: x2.into() };
        assert_eq!(parse_buttons(&b("Default", "Ctrl+Nope")), Err("X2: unknown key 'Nope'".into()));
        assert_eq!(parse_buttons(&b("run:", "Default")), Err("X1: run: needs a program, file or URL".into()));
        assert_eq!(
            parse_buttons(&b("Back", "Ctrl+Shift+T")),
            Ok((Action::Mouse(Btn::Back), Action::Keys(vec![Ctrl, Shift, Letter('T')])))
        );
    }
}
