//! Selecting several chats in the list, and the bar that acts on them.
//!
//! Ctrl (Cmd on macOS) or Shift and a click starts a selection; while one is
//! open a plain click adds or removes a chat. The selection lives in egui's
//! memory, which each account keeps its own copy of, so accounts never share
//! one. Escape or the bar's close button ends it.

use std::collections::BTreeSet;

use egui::{Rect, pos2, vec2};

use crate::app::App;
use crate::model::Action;
use crate::theme::{self, Icon, Palette};

fn id() -> egui::Id {
    egui::Id::new("fork-chat-selection")
}

fn confirm_id() -> egui::Id {
    egui::Id::new("fork-chat-selection-confirm")
}

/// The chats selected in the list.
pub fn selected(ctx: &egui::Context) -> BTreeSet<String> {
    ctx.data(|data| data.get_temp::<BTreeSet<String>>(id()))
        .unwrap_or_default()
}

fn store(ctx: &egui::Context, chats: BTreeSet<String>) {
    ctx.data_mut(|data| {
        if chats.is_empty() {
            data.remove::<BTreeSet<String>>(id());
            data.remove::<bool>(confirm_id());
        } else {
            data.insert_temp(id(), chats);
        }
    });
}

/// Handles a click on a chat row. Returns true when the click selected or
/// unselected the chat, so the row must not open it.
pub fn click(ui: &egui::Ui, chat: &str) -> bool {
    let modifiers = ui.input(|input| input.modifiers);
    let mut chats = selected(ui.ctx());
    if chats.is_empty() && !modifiers.command && !modifiers.shift {
        return false;
    }
    if !chats.remove(chat) {
        chats.insert(chat.to_owned());
    }
    store(ui.ctx(), chats);
    true
}

/// Marks a selected row: a tint over it and a check on its avatar.
pub fn paint(ui: &egui::Ui, palette: &Palette, row: Rect, avatar: Rect, chat: &str) {
    if !selected(ui.ctx()).contains(chat) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(row, 0.0, palette.accent.gamma_multiply(0.14));
    let center = pos2(avatar.right() - 7.0, avatar.bottom() - 7.0);
    painter.circle(
        center,
        9.0,
        palette.accent,
        egui::Stroke::new(2.0, palette.panel),
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        "\u{2713}",
        theme::semibold(12.0),
        palette.on_accent,
    );
}

/// The bar over the chat list while chats are selected.
pub fn bar(ctx: &egui::Context, app: &mut App) {
    let chats = selected(ctx);
    if chats.is_empty() {
        return;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        store(ctx, BTreeSet::new());
        return;
    }
    let palette = app.palette;
    let mut clear = false;
    egui::Area::new(egui::Id::new("fork-chat-selection-bar"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
        .show(ctx, |ui| {
            panel_frame(&palette).show(ui, |ui| {
                ui.set_max_width(680.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    let count = match chats.len() {
                        1 => crate::fork::i18n::tr("1 chat selected").to_owned(),
                        n => {
                            crate::fork::i18n::tr("{} chats selected").replace("{}", &n.to_string())
                        }
                    };
                    theme::text(ui, count, theme::semibold(14.0), palette.text);
                    ui.add_space(6.0);
                    let any_unarchived = chats
                        .iter()
                        .any(|chat| app.chat(chat).is_some_and(|chat| !chat.archived));
                    let any_unpinned = chats
                        .iter()
                        .any(|chat| app.chat(chat).is_some_and(|chat| !chat.pinned));
                    if button(ui, &palette, Icon::CheckCheck, "Mark as read") {
                        for chat in &chats {
                            app.actions.push(Action::MarkRead(chat.clone()));
                        }
                        clear = true;
                    }
                    let label = if any_unarchived {
                        "Archive"
                    } else {
                        "Unarchive"
                    };
                    if button(ui, &palette, Icon::Archive, label) {
                        for chat in &chats {
                            app.actions
                                .push(Action::SetArchived(chat.clone(), any_unarchived));
                        }
                        clear = true;
                    }
                    let (icon, label) = if any_unpinned {
                        (Icon::Pin, "Pin to top")
                    } else {
                        (Icon::PinOff, "Unpin")
                    };
                    if button(ui, &palette, icon, label) {
                        for chat in &chats {
                            app.actions
                                .push(Action::SetPinned(chat.clone(), any_unpinned));
                        }
                        clear = true;
                    }
                    labels_menu(ui, app, &chats);
                    if button(ui, &palette, Icon::Trash, "Delete") {
                        ctx.data_mut(|data| data.insert_temp(confirm_id(), true));
                    }
                    if button(ui, &palette, Icon::CircleX, "Cancel") {
                        clear = true;
                    }
                });
            });
        });
    if ctx
        .data(|data| data.get_temp::<bool>(confirm_id()))
        .unwrap_or(false)
    {
        clear |= confirm_delete(ctx, app, &chats);
    }
    if clear {
        store(ctx, BTreeSet::new());
    }
}

fn button(ui: &mut egui::Ui, palette: &Palette, icon: Icon, label: &str) -> bool {
    theme::soft_button(ui, palette, Some(icon), label, false).clicked()
}

/// The raised frame the bar and the fork's dialogs share.
pub fn panel_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.overlay)
        .stroke(egui::Stroke::new(1.0, palette.outline))
        .corner_radius(theme::RADIUS + 4)
        .inner_margin(16)
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: palette.shadow,
        })
}

/// Adds or removes one label on every selected chat.
fn labels_menu(ui: &mut egui::Ui, app: &mut App, chats: &BTreeSet<String>) {
    if app.labels.is_empty() {
        return;
    }
    let palette = app.palette;
    let response = theme::soft_button(ui, &palette, Some(Icon::Tag), "Labels", false);
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width(200.0);
        for label in app.labels.clone() {
            let everywhere = chats.iter().all(|chat| {
                app.chat(chat)
                    .is_some_and(|chat| chat.labels.contains(&label.id))
            });
            let mark = if everywhere { "\u{2713} " } else { "" };
            let text = egui::RichText::new(format!("{mark}{}", label.name))
                .color(crate::ui::labels::color_of(&palette, &label.color_hex));
            if ui.button(text).clicked() {
                for chat in chats {
                    let Some(current) = app.chat(chat) else {
                        continue;
                    };
                    let mut labels = current.labels.clone();
                    labels.retain(|id| id != &label.id);
                    if !everywhere {
                        labels.push(label.id.clone());
                    }
                    app.actions.push(Action::SetChatLabels {
                        chat: chat.clone(),
                        labels,
                    });
                }
            }
        }
    });
}

/// Asks before deleting; true once the chats were deleted.
fn confirm_delete(ctx: &egui::Context, app: &mut App, chats: &BTreeSet<String>) -> bool {
    let palette = app.palette;
    let mut done = false;
    let mut cancelled = false;
    egui::Modal::new(egui::Id::new("fork-chat-selection-delete"))
        .backdrop_color(palette.shadow)
        .frame(panel_frame(&palette))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            theme::text(
                ui,
                crate::fork::i18n::tr("Delete {} chats?").replace("{}", &chats.len().to_string()),
                theme::semibold(16.0),
                palette.text,
            );
            ui.label(
                egui::RichText::new(crate::fork::i18n::tr(
                    "They are deleted here and on your phone. This cannot be undone.",
                ))
                .color(palette.secondary),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if button(ui, &palette, Icon::Trash, "Delete") {
                    for chat in chats {
                        app.actions.push(Action::DeleteChat(chat.clone()));
                    }
                    done = true;
                }
                if theme::soft_button(ui, &palette, None, "Cancel", false).clicked() {
                    cancelled = true;
                }
            });
        });
    if cancelled {
        ctx.data_mut(|data| data.remove::<bool>(confirm_id()));
    }
    done
}
