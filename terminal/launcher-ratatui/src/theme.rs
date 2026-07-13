//! Theme palette + system light/dark detection.

use std::{
    env,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use ratatui::style::Color;

use crate::AMBER;

/// Palette that stays readable on both terminal backgrounds.
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub text: Color,
    pub muted: Color,
    pub dim: Color,
    pub key: Color,
    pub border: Color,
    /// Unselected agent chip / list row.
    pub soft: Color,
    /// Selected row full-width fill (one step off bg).
    pub surface: Color,
    /// Git dirty / ahead — never share hue with ACCENT.
    pub warn: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(20, 20, 20),
            text: Color::White,
            muted: Color::Gray,
            dim: Color::DarkGray,
            key: Color::White,
            border: Color::DarkGray,
            soft: Color::Gray,
            surface: Color::Rgb(38, 38, 38), // #262626
            warn: AMBER,
        }
    }

    pub fn light() -> Self {
        // Stronger contrast muted text (zinc-ish); needs truecolor (Ghostty / xterm.js).
        Self {
            bg: Color::Rgb(250, 250, 250),
            text: Color::Rgb(23, 23, 23),
            muted: Color::Rgb(82, 82, 91),   // zinc-600
            dim: Color::Rgb(113, 113, 122), // zinc-500
            key: Color::Rgb(24, 24, 27),
            border: Color::Rgb(161, 161, 170),
            soft: Color::Rgb(63, 63, 70), // zinc-700
            surface: Color::Rgb(236, 236, 236), // #ececec
            warn: Color::Rgb(161, 98, 7),       // darker amber on light
        }
    }

    pub fn from_name(name: &str) -> Self {
        match resolved_theme_mode(name) {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }
}

/// Resolve preference to concrete `"light"` | `"dark"`.
pub fn resolved_theme_mode(preference: &str) -> &'static str {
    if preference.eq_ignore_ascii_case("light") {
        return "light";
    }
    if preference.eq_ignore_ascii_case("dark") {
        return "dark";
    }
    // auto (or unknown) — detect terminal / OS.
    if detect_system_is_light() {
        "light"
    } else {
        "dark"
    }
}

pub fn format_theme_label(preference: &str) -> String {
    if preference.eq_ignore_ascii_case("auto")
        || (!preference.eq_ignore_ascii_case("light")
            && !preference.eq_ignore_ascii_case("dark"))
    {
        format!("auto ({})", resolved_theme_mode("auto"))
    } else {
        preference.to_ascii_lowercase()
    }
}

/// Best-effort light/dark detection for `ui_theme = auto`.
/// Order: MC_UI_THEME → COLORFGBG → macOS appearance → dark.
fn detect_system_is_light() -> bool {
    if let Ok(v) = env::var("MC_UI_THEME") {
        let v = v.trim();
        if v.eq_ignore_ascii_case("light") {
            return true;
        }
        if v.eq_ignore_ascii_case("dark") {
            return false;
        }
    }

    if let Some(is_light) = colorfgbg_is_light() {
        return is_light;
    }

    #[cfg(target_os = "macos")]
    {
        return macos_appearance_is_light();
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Parse `COLORFGBG` (e.g. `15;0` = light fg / dark bg). Returns None if unset/unparseable.
fn colorfgbg_is_light() -> Option<bool> {
    let v = env::var("COLORFGBG").ok()?;
    let bg = v
        .split([';', ':'])
        .filter(|s| !s.is_empty())
        .last()?
        .trim()
        .parse::<u16>()
        .ok()?;
    // xterm convention: 7 and 15 are light backgrounds; 0–6 / 8–14 are dark-ish.
    Some(bg == 7 || bg == 15)
}

#[cfg(target_os = "macos")]
fn macos_appearance_is_light() -> bool {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

    if let Ok(guard) = CACHE.lock() {
        if let Some((at, is_light)) = *guard {
            if at.elapsed() < Duration::from_secs(5) {
                return is_light;
            }
        }
    }

    // `AppleInterfaceStyle` is "Dark" when dark; the key is missing in light mode.
    let is_light = match Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .output()
    {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            !s.trim().eq_ignore_ascii_case("Dark")
        }
        Err(_) => false, // fall back dark if defaults unavailable
    };

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((Instant::now(), is_light));
    }
    is_light
}
