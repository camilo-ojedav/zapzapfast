//! The fork's own credit, drawn beside upstream's in the About section.

use crate::theme::{self, Palette};

/// Who keeps this fork.
pub const FORK_AUTHOR: &str = "Camilo Ojeda";
/// Where the credit links to.
pub const FORK_URL: &str = "https://github.com/camilo-ojedav/zapzapfast";

/// Appends "· fork by Camilo Ojeda" to the credit line it is called inside.
pub fn credit(ui: &mut egui::Ui, palette: &Palette) {
    theme::text(ui, " \u{b7} ", theme::regular(13.0), palette.secondary);
    theme::text(ui, "Fork by ", theme::regular(13.0), palette.secondary);
    if theme::link(ui, FORK_AUTHOR, theme::medium(13.0), palette.link)
        .on_hover_text(FORK_URL)
        .clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(FORK_URL));
    }
}
