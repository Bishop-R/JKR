//! Shared content-card layout helpers.

use super::{ContentCard, MenuCanvas};
use jkr_ui::{FontWeight, Rect};

impl MenuCanvas {
    /// Lay out a compact centered card whose height is exactly its content plus padding.
    pub(crate) fn centered_card(
        &self,
        preferred_width: f32,
        content_height: f32,
        padding: f32,
    ) -> ContentCard {
        let width = preferred_width
            .clamp(420.0, 560.0)
            .min((self.viewport[0] - 64.0).max(1.0));
        let height = content_height + padding * 2.0;
        let panel = Rect::new(
            (self.viewport[0] - width) * 0.5,
            (self.viewport[1] - height) * 0.5,
            width,
            height,
        );
        ContentCard {
            panel,
            content: Rect::new(
                panel.x + padding,
                panel.y + padding,
                panel.width - padding * 2.0,
                content_height,
            ),
            padding,
        }
    }

    /// Draw the title hierarchy inside a content-sized card and return the next content y.
    pub(crate) fn card_heading(
        &mut self,
        card: ContentCard,
        kicker: &str,
        title: &str,
        subtitle: &str,
    ) -> f32 {
        let content = card.content;
        self.text(
            kicker,
            Rect::new(content.x, content.y, content.width, 18.0),
            11.0,
            self.theme.accent,
            FontWeight::Semibold,
            1.7,
        );
        self.text(
            title,
            Rect::new(content.x, content.y + 24.0, content.width, 42.0),
            32.0,
            self.theme.foreground,
            FontWeight::Semibold,
            0.0,
        );
        self.text(
            subtitle,
            Rect::new(content.x, content.y + 68.0, content.width, 24.0),
            14.0,
            self.theme.muted,
            FontWeight::Regular,
            0.0,
        );
        content.y + 112.0
    }
}
