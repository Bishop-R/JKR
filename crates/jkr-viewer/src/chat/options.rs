//! Legacy chat options mapped onto the floating, retained conversation layout.
use super::*;
use crate::{cgame_options::scalar, console::ViewerConsole};

#[derive(Clone, Copy, PartialEq)]
/// Bounded presentation settings read from the archived cvars.
pub(super) struct Options {
    /// Milliseconds before a message expires.
    pub(super) lifetime: u64,
    /// Maximum retained conversation entries displayed together.
    pub(super) lines: usize,
    /// Multiplier for the modern conversation font.
    pub(super) font: f32,
    /// Left edge in virtual 640-wide coordinates.
    pub(super) x: f32,
    /// Bottom edge in virtual 480-high coordinates.
    pub(super) height: f32,
    /// Wrapping width in virtual coordinates.
    pub(super) width: f32,
    /// Expired history requested while the console catcher is active.
    pub(super) history: bool,
    /// Incoming body colour-cleaning mode.
    pub(super) clean: i64,
    /// Centre-print lifetime in milliseconds.
    pub(super) center_time: u64,
    /// Centre-print size multiplier.
    pub(super) center_size: f32,
    /// Centre-print virtual Y, with zero selecting the hero default.
    pub(super) center_height: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self::read(None)
    }
}

impl Options {
    fn read(console: Option<&ViewerConsole>) -> Self {
        let integer = |name, default| {
            console
                .and_then(|c| c.integer_cvar(name))
                .unwrap_or(default)
        };
        Self {
            lifetime: integer("cg_chatbox", 10000).max(0) as u64,
            lines: integer("cg_chatboxlines", 5).clamp(1, MAX_VISIBLE as i64) as usize,
            font: scalar(console, "cg_chatboxfontsize", 1.0).clamp(0.25, 3.0),
            x: scalar(console, "cg_chatboxx", 30.0).clamp(0.0, 600.0),
            height: scalar(console, "cg_chatboxheight", 350.0).clamp(0.0, 480.0),
            width: scalar(console, "cg_chatboxcutofflength", 350.0).clamp(60.0, 640.0),
            history: integer("cg_chatboxshowhistory", 0) != 0
                && console.is_some_and(ViewerConsole::is_open),
            clean: integer("cg_cleanchatbox", 0),
            center_time: (scalar(console, "cg_centertime", 3.0).clamp(0.0, 3600.0) * 1000.0) as u64,
            center_size: scalar(console, "cg_centersize", 1.0).clamp(0.0, 4.0),
            center_height: scalar(console, "cg_centerheight", 0.0).clamp(0.0, 480.0),
        }
    }

    /// Transform legacy coordinate controls into the retained modern layout.
    pub(super) fn geometry(self, viewport: [f32; 2]) -> layout::Geometry {
        let mut g = layout::Geometry::new(viewport);
        g.left = self.x / 640.0 * viewport[0];
        g.bottom = self.height / 480.0 * viewport[1];
        g.top = 0.0;
        g.width = (self.width / 640.0 * viewport[0])
            .min(viewport[0] - g.left)
            .max(1.0);
        g.font *= self.font;
        g.row *= self.font;
        g.scale *= self.font;
        g
    }
}

/// Clean message colour markup on arrival, leaving the separately rendered name intact.
pub(super) fn clean_body(body: &str, mode: i64) -> String {
    if mode == 1 {
        let mut output = String::with_capacity(body.len());
        let mut chars = body.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '^' && chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                chars.next();
            } else {
                output.push(ch);
            }
        }
        output
    } else if mode > 1 {
        let mut rest = body;
        while rest.as_bytes().first() == Some(&b'^')
            && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
        {
            rest = &rest[2..];
        }
        rest.to_owned()
    } else {
        body.to_owned()
    }
}

impl ChatOverlay {
    /// Console history can draw while the rest of the gameplay HUD is suppressed.
    pub(crate) fn wants_history(&self, console: Option<&ViewerConsole>) -> bool {
        console.is_some_and(|c| {
            c.is_open() && c.integer_cvar("cg_chatboxshowhistory").unwrap_or(0) != 0
        })
    }
    /// Apply cvars without allocating; wrapping invalidates through width/font keys.
    pub(crate) fn configure(&mut self, console: Option<&ViewerConsole>) {
        if let Some(console) = console {
            self.friends.initialize(console.config_directory());
        }
        let options = Options::read(console);
        if self.options != options {
            for line in &mut self.lines {
                line.y = None;
            }
            self.options = options;
        }
    }
}
