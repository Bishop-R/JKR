//! Retained drop-down console presentation; command behavior remains in `console.rs`.

use super::console_text::ConsoleText;
use super::edit_view::{EditFrame, OutputRows};
use super::selection::Mark;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextStyle, TextVertex, UiFont};
use jkr_shell::{ConsoleLine, ConsoleLineKind};
use jkr_ui::{Color, DrawList, FontWeight, InputEvent, Rect, TextAlign, UiEventKind};

/// Fixed-storage console draw model.
pub(crate) struct ConsolePresentation {
    ui: MenuCanvas,
    options: super::console_options::Options,
    fraction: f32,
    tick: std::time::Instant,
    header: String,
    header_second: u64,
}

impl ConsolePresentation {
    pub(crate) fn new() -> Self {
        Self {
            ui: MenuCanvas::new(),
            options: super::console_options::Options {
                height: 0.58,
                ..Default::default()
            },
            fraction: 0.0,
            tick: std::time::Instant::now(),
            header: "CONSOLE".into(),
            header_second: 0,
        }
    }
    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn append<'a>(
        &mut self,
        lines: impl DoubleEndedIterator<Item = &'a ConsoleLine>,
        configured_lines: usize,
        scroll_offset: usize,
        edit: EditFrame<'_>,
        completion: &str,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        // Other overlays share this bounded batch. Give the console's fixed
        // controls priority even when chat or diagnostics filled the text budget.
        let reserve = (edit
            .prompt
            .input
            .len()
            .saturating_add(completion.len())
            .saturating_add(160))
        .saturating_mul(12)
        .min(crate::text::MAX_TEXT_VERTICES);
        let keep = vertices.len().min(crate::text::MAX_TEXT_VERTICES - reserve);
        vertices.truncate(keep / 6 * 6);
        build_options(
            &mut self.ui,
            lines.map(|line| {
                (
                    line.kind,
                    if self.options.timestamps != 0 {
                        line.stamped_text.as_str()
                    } else {
                        line.text.as_str()
                    },
                )
            }),
            configured_lines,
            scroll_offset,
            edit,
            completion,
            font,
            viewport,
            self.options,
            &self.header,
        );
        self.ui
            .append_text_styled(vertices, font, viewport, TextStyle::NEUTRAL);
    }

    pub(crate) fn pointer(&mut self, event: InputEvent) -> Option<f32> {
        let event = self.ui.pointer(event)?;
        (event.kind == UiEventKind::Wheel).then(|| event.delta.map_or(0.0, |delta| delta.y))
    }

    pub(super) fn append_options<'a>(
        &mut self,
        lines: impl DoubleEndedIterator<Item = &'a ConsoleLine>,
        configured: usize,
        scroll: usize,
        edit: EditFrame<'_>,
        completion: &str,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        options: super::console_options::Options,
        open: bool,
        now: u64,
    ) {
        let delta = self.tick.elapsed().as_secs_f32();
        self.tick = std::time::Instant::now();
        let target = if open { options.height } else { 0.0 };
        let step = options.speed * delta;
        self.fraction += (target - self.fraction).clamp(-step, step);
        self.options = options;
        self.options.height = self.fraction;
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if options.datetime && (seconds != self.header_second || self.header == "CONSOLE") {
            self.header = super::console_options::datetime(seconds);
            self.header_second = seconds;
        } else if !options.datetime && self.header != "CONSOLE" {
            self.header.clear();
            self.header.push_str("CONSOLE");
        }
        if self.fraction > 0.08 {
            self.append(
                lines, configured, scroll, edit, completion, vertices, font, viewport,
            );
        } else {
            self.ui.begin_transparent(viewport);
            if !open {
                let scale = (viewport[1] / 1080.0).clamp(0.75, 2.5) * options.scale;
                let color = self.ui.theme().foreground;
                let size = 14.0 * scale;
                let pitch = size * options.line_spacing;
                for (i, line) in lines
                    .rev()
                    .filter(|line| now.saturating_sub(line.written_millis) < options.notify_millis)
                    .take(options.notify_lines)
                    .enumerate()
                {
                    let text = if options.timestamps == 1 {
                        &line.stamped_text
                    } else {
                        &line.text
                    };
                    self.ui.text(
                        text,
                        Rect::new(
                            12.0 * scale + options.notify_x * viewport[0] / 640.0,
                            12.0 * scale + (options.notify_lines - i - 1) as f32 * pitch,
                            viewport[0] - 24.0 * scale,
                            pitch,
                        ),
                        size,
                        color,
                        FontWeight::Regular,
                        options.tracking * size,
                    );
                }
            }
            self.ui.finish(u16::MAX);
            self.ui
                .append_text_styled(vertices, font, viewport, TextStyle::NEUTRAL);
        }
    }
}

fn build_options<'a>(
    ui: &mut MenuCanvas,
    lines: impl DoubleEndedIterator<Item = (ConsoleLineKind, &'a str)>,
    configured_lines: usize,
    scroll_offset: usize,
    edit: EditFrame<'_>,
    completion: &str,
    font: &UiFont,
    viewport: [f32; 2],
    options: super::console_options::Options,
    header: &str,
) {
    ui.begin_transparent(viewport);
    let scale = (viewport[1] / 1080.0).clamp(0.75, 2.5) * options.scale;
    let height = (viewport[1] * options.height).min(viewport[1]);
    let margin = 24.0 * scale;
    let width = (viewport[0] - margin * 2.0).max(0.0);
    ui.column_tint_opacity(Rect::new(0.0, 0.0, viewport[0], height), options.opacity);
    let theme = ui.theme();
    // Console text is laid out with the player's letter spacing here, where its
    // positions are decided, rather than added when glyphs are emitted.
    let spacing = |size: f32| options.tracking * size;
    ui.text(
        header,
        Rect::new(margin, 12.0 * scale, width, 20.0 * scale),
        12.0 * scale,
        theme.muted,
        FontWeight::Semibold,
        1.0 * scale + spacing(12.0 * scale),
    );
    // Hide the secondary hint when it would compete with the title.
    if width >= 440.0 * scale {
        ui.text_aligned(
            if scroll_offset > 0 {
                "Scrollback  /  Scroll down for latest"
            } else {
                "Tab complete   /   Up, Down history   /   F3 browse"
            },
            Rect::new(
                margin + 120.0 * scale,
                12.0 * scale,
                width - 120.0 * scale,
                20.0 * scale,
            ),
            12.0 * scale,
            theme.muted,
            FontWeight::Regular,
            spacing(12.0 * scale),
            TextAlign::End,
        );
    }
    let input_y = height - 62.0 * scale;
    let line_height = 14.0 * scale * options.line_spacing;
    let top = 44.0 * scale;
    let bottom = input_y - 10.0 * scale;
    let available = ((bottom - top) / line_height).max(0.0) as usize;
    let maximum = configured_lines.max(1).min(available);
    let EditFrame {
        prompt,
        selection,
        lines_end,
    } = edit;
    // Text selection: the scrollback above the separator, the input line below it.
    selection.begin_frame(
        Rect::new(0.0, top, viewport[0], (input_y - top).max(0.0)),
        Rect::new(0.0, input_y, viewport[0], (height - input_y).max(0.0)),
    );
    // The input line and scrollback rows are drawn, measured and hit-tested with
    // these, so carets, highlights and pointer hits follow any size or spacing.
    let input_text = ConsoleText::new(font, 15.0 * scale, options.tracking);
    let row_text = ConsoleText::new(font, 14.0 * scale, options.tracking);
    // Submit the fixed input before history so a full glyph budget cannot hide it.
    ui.separator(Rect::new(margin, input_y, width, 1.0));
    super::edit_view::prompt(
        ui,
        input_text,
        Rect::new(margin, input_y + 8.0 * scale, width, 24.0 * scale),
        &prompt,
        selection,
    );
    ui.text(
        if completion.is_empty() {
            "cmdlist lists commands"
        } else {
            completion
        },
        Rect::new(margin, height - 23.0 * scale, width, 18.0 * scale),
        12.0 * scale,
        theme.muted,
        FontWeight::Regular,
        spacing(12.0 * scale),
    );

    ui.scroll_region(0, Rect::new(margin, top, width, (bottom - top).max(0.0)));
    let mut rows = OutputRows::new(ui, row_text, selection, line_height, bottom);
    for (index, (kind, line, start, text)) in lines
        .rev()
        .zip((0..lines_end).rev())
        .flat_map(|((kind, text), line)| {
            // Rows are slices of their line, so a row's address gives its byte offset.
            text.split('\n').rev().map(move |row| {
                let start = row.as_ptr() as usize - text.as_ptr() as usize;
                (kind, line, start, row)
            })
        })
        .skip(scroll_offset)
        .take(maximum)
        .enumerate()
    {
        let color = if kind == ConsoleLineKind::Error {
            Color::new(1.0, 0.55, 0.52, 1.0)
        } else {
            theme.foreground
        };
        let rect = Rect::new(
            margin,
            bottom - (index + 1) as f32 * line_height,
            width,
            line_height,
        );
        rows.row(ui, index, Mark { line, byte: start }, text, rect, color);
    }
    rows.finish();
    selection.end_frame();
    ui.finish(u16::MAX);
}
