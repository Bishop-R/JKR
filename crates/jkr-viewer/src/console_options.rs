//! OpenJK cl_console.cpp:492-502,610-612,737-773,1038-1067.
//! notifylines/datetime extend it with TaystJK cl_console.cpp:650-668,1090.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub height: f32,
    pub scale: f32,
    pub opacity: f32,
    pub speed: f32,
    pub timestamps: i64,
    pub datetime: bool,
    pub notify_millis: u64,
    pub notify_lines: usize,
    /// Notify-only horizontal displacement, in virtual 640-wide coordinates.
    pub notify_x: f32,
    /// History and notify row pitch as a multiple of their text size
    /// (`con_lineSpacing`).
    pub line_spacing: f32,
    /// Extra advance after every glyph as a fraction of the text size
    /// (`ui_letterSpacing`), applied where the console lays out its text.
    pub tracking: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            height: 0.5,
            scale: 1.0,
            opacity: 1.0,
            speed: 3.0,
            timestamps: 0,
            datetime: false,
            notify_millis: 3000,
            notify_lines: 3,
            notify_x: 0.0,
            line_spacing: LINE_SPACING_DEFAULT,
            tracking: 0.0,
        }
    }
}

pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for (name, value, help) in [
        ("con_notifytime", 3.0, "Notify lifetime in seconds"),
        ("con_opacity", 1.0, "Console background opacity"),
        ("con_scale", 1.0, "Console font scale"),
        (
            "con_lineSpacing",
            f64::from(LINE_SPACING_DEFAULT),
            "Console row pitch as a multiple of the text size (1 to 2)",
        ),
        (
            "con_height",
            0.5,
            "Open console height as a screen fraction",
        ),
        ("scr_conspeed", 3.0, "Console opening/closing speed"),
    ] {
        let flags = if matches!(name, "con_notifytime" | "scr_conspeed") {
            CvarFlags::NONE
        } else {
            CvarFlags::ARCHIVE
        };
        cvars.register(CvarDefinition::new(name, value, flags, help))?;
    }
    for (name, value, help) in [
        ("con_notifylines", 3_i64, "Maximum visible notify lines"),
        (
            "con_timestamps",
            0,
            "Timestamps: 0 off, 1 console and notify, 2 console only",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for (name, value, help) in [
        ("con_autoclear", true, "Clear console input when closing"),
        ("cl_noprint", false, "Suppress console output"),
        (
            "con_datetime",
            false,
            "Show current UTC date/time in the console header",
        ),
    ] {
        let flags = if name == "cl_noprint" {
            CvarFlags::NONE
        } else {
            CvarFlags::ARCHIVE
        };
        cvars.register(CvarDefinition::new(name, value, flags, help))?;
    }
    Ok(())
}

impl ViewerConsole {
    pub(super) fn options(&self) -> Options {
        Options {
            notify_x: self.float_cvar("cl_conxoffset").unwrap_or(0.0) as f32,
            height: self.float_cvar("con_height").unwrap_or(0.5).clamp(0.0, 1.0) as f32,
            scale: self
                .float_cvar("con_scale")
                .filter(|value| *value > 0.0)
                .unwrap_or(1.0) as f32,
            opacity: self
                .float_cvar("con_opacity")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0) as f32,
            speed: self
                .float_cvar("scr_conspeed")
                .unwrap_or(3.0)
                .clamp(1.0, 100.0) as f32,
            timestamps: self.integer_cvar("con_timestamps").unwrap_or(0),
            datetime: self.bool_cvar("con_datetime").unwrap_or(false),
            notify_millis: (self.float_cvar("con_notifytime").unwrap_or(3.0).max(0.0) * 1000.0)
                as u64,
            notify_lines: self
                .integer_cvar("con_notifylines")
                .unwrap_or(3)
                .clamp(0, 64) as usize,
            line_spacing: line_spacing(self.float_cvar("con_lineSpacing")),
            tracking: crate::text::TextStyle::from_cvars(
                None,
                self.float_cvar(crate::text::style::TRACKING_CVAR),
            )
            .tracking,
        }
    }
}

/// Default `con_lineSpacing`: 14 px rows 16.1 px apart at 1080p, stock's 16 px
/// console row pitch, which leaves Inter about 1.4 em of leading.
pub(super) const LINE_SPACING_DEFAULT: f32 = 1.15;

/// Accepted `con_lineSpacing` range. At the lower end rows touch: the line
/// pitch equals the text's line box.
pub(super) const LINE_SPACING_RANGE: (f32, f32) = (1.0, 2.0);

/// Clamped `con_lineSpacing`; missing or non-finite values keep the default.
pub(super) fn line_spacing(value: Option<f64>) -> f32 {
    value
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .map_or(LINE_SPACING_DEFAULT, |value| {
            value.clamp(LINE_SPACING_RANGE.0, LINE_SPACING_RANGE.1)
        })
}

/// Gregorian UTC date from Unix days (no OS-specific code in the console).
pub(super) fn datetime(seconds: u64) -> String {
    let days = (seconds / 86400) as i64 + 719468;
    let era = days / 146097;
    let day = days - era * 146097;
    let year = (day - day / 1460 + day / 36524 - day / 146096) / 365;
    let ordinal = day - (365 * year + year / 4 - year / 100);
    let month = (5 * ordinal + 2) / 153;
    let d = ordinal - (153 * month + 2) / 5 + 1;
    let m = month + if month < 10 { 3 } else { -9 };
    let y = year + era * 400 + i64::from(m <= 2);
    format!(
        "CONSOLE  {y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_spacing_defaults_and_clamps() {
        assert_eq!(line_spacing(None), LINE_SPACING_DEFAULT);
        assert_eq!(line_spacing(Some(f64::NAN)), LINE_SPACING_DEFAULT);
        assert_eq!(line_spacing(Some(1.25)), 1.25);
        assert_eq!(line_spacing(Some(0.1)), LINE_SPACING_RANGE.0);
        assert_eq!(line_spacing(Some(9.0)), LINE_SPACING_RANGE.1);
    }
}
