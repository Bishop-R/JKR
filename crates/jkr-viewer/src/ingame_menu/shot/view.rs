//! Responsive right-edge panel; the rest of the world stays untinted.
use super::*;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

struct Layout {
    s: f32,
    x: f32,
}
impl Layout {
    fn new(viewport: [f32; 2]) -> Self {
        let s = (viewport[1] / 740.).min(viewport[0] / 440.).clamp(
            0.6,
            crate::ui_scale::MAX * crate::ui_scale::REFERENCE_HEIGHT / 740.,
        );
        Self {
            s,
            x: viewport[0] - 412. * s,
        }
    }
    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            self.x + x * self.s,
            (y + 16.) * self.s,
            w * self.s,
            h * self.s,
        )
    }
}

pub(super) fn build(panel: &Panel, ui: &mut MenuCanvas, viewport: [f32; 2]) {
    let l = Layout::new(viewport);
    ui.begin_transparent(viewport);
    let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: l.r(0., 0., 396., 704.),
        radius: 12. * l.s,
        color: Color::new(0.012, 0.022, 0.035, 0.94),
    });
    label(
        ui,
        "SHOT CONTROLS",
        l.r(20., 18., 320., 30.),
        24. * l.s,
        true,
    );
    label(
        ui,
        "Frame the scene, then hide the panel.",
        l.r(20., 54., 356., 22.),
        14. * l.s,
        false,
    );
    button(
        ui,
        panel,
        CAMERA,
        "Camera",
        l.r(20., 90., 174., 36.),
        l.s,
        !panel.sun_tab,
    );
    button(
        ui,
        panel,
        SUN,
        "Sun",
        l.r(202., 90., 174., 36.),
        l.s,
        panel.sun_tab,
    );
    if panel.sun_tab {
        sun(ui, panel, &l);
    } else {
        camera(ui, panel, &l);
    }
    shared(ui, panel, &l);
    ui.finish(panel.selected);
}

fn camera(ui: &mut MenuCanvas, panel: &Panel, l: &Layout) {
    for (i, name) in ["Back", "Front", "Left", "Right"].into_iter().enumerate() {
        button(
            ui,
            panel,
            12 + i as u16,
            name,
            l.r(20. + i as f32 * 91., 142., 83., 32.),
            l.s,
            false,
        );
    }
    for i in 0..4 {
        slider(
            ui,
            panel,
            i,
            l.r(20., 188. + i as f32 * 48., 356., 44.),
            l.s,
        );
    }
}

fn sun(ui: &mut MenuCanvas, panel: &Panel, l: &Layout) {
    if panel.sun_available {
        slider(ui, panel, 4, l.r(20., 152., 356., 46.), l.s);
        slider(ui, panel, 5, l.r(20., 210., 356., 46.), l.s);
        label(
            ui,
            "0° = horizon · 90° = overhead",
            l.r(20., 276., 356., 22.),
            14. * l.s,
            false,
        );
        label(
            ui,
            match panel.sun_mode {
                crate::console::director::SunMode::Automatic => "Automatic · using day settings",
                crate::console::director::SunMode::Manual => "Manual · shot controls own the sun",
                crate::console::director::SunMode::Returning => "Returning to day settings…",
            },
            l.r(20., 310., 356., 22.),
            14. * l.s,
            false,
        );
    } else {
        label(
            ui,
            "Sun controls unavailable in this view.",
            l.r(20., 164., 356., 22.),
            14. * l.s,
            false,
        );
        label(
            ui,
            "Requires day/night and an outdoor sky.",
            l.r(20., 198., 356., 22.),
            14. * l.s,
            false,
        );
    }
    button(
        ui,
        panel,
        RESET_SUN,
        "Reset sun · use day settings",
        l.r(20., 346., 356., 36.),
        l.s,
        false,
    );
}

fn shared(ui: &mut MenuCanvas, panel: &Panel, l: &Layout) {
    let available = !panel.sun_tab || panel.sun_available;
    slider(ui, panel, 6, l.r(20., 400., 356., 44.), l.s);
    if available {
        slider(ui, panel, 7, l.r(20., 452., 356., 44.), l.s);
    }
    button(
        ui,
        panel,
        LIVE,
        if panel.live {
            "Live preview: ON"
        } else {
            "Live preview: OFF"
        },
        l.r(20., 512., 222., 36.),
        l.s,
        panel.live,
    );
    button(
        ui,
        panel,
        HUD,
        if panel.hud { "HUD: ON" } else { "HUD: OFF" },
        l.r(250., 512., 126., 36.),
        l.s,
        panel.hud,
    );
    if available {
        button(
            ui,
            panel,
            ORBIT,
            "Orbit",
            l.r(20., 560., 112., 36.),
            l.s,
            false,
        );
        button(
            ui,
            panel,
            STOP,
            "Stop",
            l.r(140., 560., 112., 36.),
            l.s,
            false,
        );
        button(
            ui,
            panel,
            RESET,
            if panel.sun_tab { "Ease back" } else { "Reset" },
            l.r(260., 560., 116., 36.),
            l.s,
            false,
        );
    }
    button(
        ui,
        panel,
        APPLY_HIDE,
        if panel.live {
            "Hide panel"
        } else {
            "Move + hide"
        },
        l.r(20., 610., 222., 40.),
        l.s,
        true,
    );
    button(
        ui,
        panel,
        HIDE,
        "Close",
        l.r(250., 610., 126., 40.),
        l.s,
        false,
    );
    label(
        ui,
        "F8 / Esc hides · Arrows fine-tune sliders",
        l.r(20., 668., 356., 20.),
        13. * l.s,
        false,
    );
}

fn label(ui: &mut MenuCanvas, text: &str, rect: Rect, size: f32, strong: bool) {
    ui.text(
        text,
        rect,
        size,
        if strong {
            ui.theme().foreground
        } else {
            Color::new(0.72, 0.80, 0.88, 1.)
        },
        if strong {
            FontWeight::Semibold
        } else {
            FontWeight::Regular
        },
        0.,
    );
}

#[allow(clippy::too_many_arguments)]
fn button(
    ui: &mut MenuCanvas,
    panel: &Panel,
    token: u16,
    text: &str,
    rect: Rect,
    s: f32,
    selected: bool,
) {
    let focus = panel.selected == token;
    if selected || focus || ui.token_hovered(token) {
        ui.accent_sweep(rect, 0.12, 0.);
    }
    if focus {
        let accent = ui.theme().accent;
        let _ = ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 2. * s,
            width: s,
            color: accent,
        });
    }
    ui.separator_line(Rect::new(rect.x, rect.bottom() - 1., rect.width, 1.));
    ui.hit_region(token, rect);
    ui.text_aligned(
        text,
        Rect::new(rect.x, rect.y + 9. * s, rect.width, 23. * s),
        15. * s,
        if selected {
            ui.theme().accent
        } else {
            ui.theme().foreground
        },
        FontWeight::Semibold,
        0.,
        TextAlign::Center,
    );
}

fn slider(ui: &mut MenuCanvas, panel: &Panel, row: usize, rect: Rect, s: f32) {
    let (name, min, max, _) = SLIDERS[row];
    if panel.selected == row as u16
        || panel.selected == crate::menu_widgets::numeric::VALUE_BASE + row as u16
    {
        ui.accent_sweep(rect, 0.12, 0.);
    }
    label(
        ui,
        name,
        Rect::new(rect.x, rect.y, rect.width * 0.65, 20. * s),
        15. * s,
        false,
    );
    let suffix = if row == 6 {
        " s"
    } else if row == 7 {
        "°/s"
    } else if matches!(row, 0 | 1 | 4 | 5) {
        "°"
    } else {
        ""
    };
    let value_rect = Rect::new(rect.right() - 104.0 * s, rect.y, 104.0 * s, 20.0 * s);
    ui.hit_region(
        crate::menu_widgets::numeric::VALUE_BASE + row as u16,
        value_rect,
    );
    if let Some(edit) = panel.numeric.as_ref().filter(|edit| edit.row == row) {
        edit.draw(ui, value_rect, s);
    } else {
        ui.text_fmt_aligned(
            format_args!("{}{suffix}", panel.values[row]),
            value_rect,
            15.0 * s,
            ui.theme().foreground,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
    }
    let rail = Rect::new(
        rect.x + 6. * s,
        rect.y + 22. * s,
        rect.width - 12. * s,
        22. * s,
    );
    ui.slider_rail(
        rail,
        (panel.values[row] - min) / (max - min),
        ui.theme().accent,
        s,
    );
    ui.hit_region(row as u16, rail);
}
