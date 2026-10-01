//! Input-facing methods kept separate from visual widget construction.

use super::*;
use crate::audio::ui_cues;
use jkr_ui::{AbstractAction, InputEvent, UiEventKind, Vec2, Widget, WidgetId};

/// One renderer-independent pointer result resolved to a screen token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasEvent {
    pub(crate) token: Option<MenuToken>,
    pub(crate) kind: UiEventKind,
    pub(crate) position: Option<Vec2>,
    pub(crate) delta: Option<Vec2>,
}

impl MenuCanvas {
    /// Route keyboard/gamepad navigation through `jkr-ui`.
    pub(crate) fn action(&mut self, action: AbstractAction) -> Option<MenuToken> {
        let first_new_event = self.input.events().len();
        self.input
            .route(InputEvent::Action(action), &self.tree, &self.rects);
        self.post_cues(first_new_event);
        self.semantic_event()
    }

    /// Route one normalized pointer event through the retained widget tree.
    pub(crate) fn pointer(&mut self, event: InputEvent) -> Option<CanvasEvent> {
        let first_new_event = self.input.events().len();
        self.input.route(event, &self.tree, &self.rects);
        self.post_cues(first_new_event);
        self.input
            .events()
            .get(first_new_event..)?
            .last()
            .map(|event| CanvasEvent {
                token: event
                    .target
                    .and_then(|id| self.tokens.get(id.0 as usize).copied()),
                kind: event.kind,
                position: event.position,
                delta: event.delta,
            })
    }

    /// Post an interface sound cue for each newly routed event that a
    /// listener would expect to hear: focus or hover arriving on a control,
    /// an activation, a cancel.
    fn post_cues(&self, first_new_event: usize) {
        for event in self.input.events().iter().skip(first_new_event) {
            let focusable = event
                .target
                .and_then(|id| self.tree.get(id))
                .is_some_and(|widget| widget.focusable);
            let cue = match event.kind {
                UiEventKind::HoverEnter | UiEventKind::Focus if focusable => ui_cues::Cue::Hover,
                UiEventKind::Activate => ui_cues::Cue::Click,
                UiEventKind::Cancel => ui_cues::Cue::Back,
                _ => continue,
            };
            ui_cues::post(cue);
        }
    }

    pub(crate) fn scroll_region(&mut self, token: MenuToken, rect: Rect) {
        self.interactive(token, rect, false, true);
    }

    pub(crate) fn hit_region(&mut self, token: MenuToken, rect: Rect) {
        self.interactive(token, rect, true, false);
    }

    pub(crate) fn rect_for(&self, token: MenuToken) -> Option<Rect> {
        self.tokens
            .iter()
            .position(|candidate| *candidate == token)
            .and_then(|index| self.rects.get(index).copied())
    }

    pub(super) fn interactive(
        &mut self,
        token: MenuToken,
        rect: Rect,
        focusable: bool,
        scrollable: bool,
    ) {
        if self.tokens.len() >= MAX_WIDGETS {
            return;
        }
        let id = WidgetId(self.tokens.len() as u32);
        let _ = self.tree.add(Widget {
            id,
            parent: None,
            layout: Default::default(),
            size: Default::default(),
            visible: true,
            focusable,
            scrollable,
            opacity: 1.0,
            z: 10,
        });
        self.tokens.push(token);
        self.rects.push(rect);
    }

    /// Whether the pointer currently rests on `token`'s target.
    pub(crate) fn token_hovered(&self, token: MenuToken) -> bool {
        self.hovered_token == Some(token)
    }

    pub(super) fn token_pressed(&self, token: MenuToken) -> bool {
        self.pressed_token == Some(token)
    }

    fn semantic_event(&self) -> Option<MenuToken> {
        let event = self.input.events().last()?;
        matches!(
            event.kind,
            UiEventKind::Activate | UiEventKind::Focus | UiEventKind::Hover
        )
        .then(|| {
            event
                .target
                .and_then(|id| self.tokens.get(id.0 as usize).copied())
        })
        .flatten()
    }
}
