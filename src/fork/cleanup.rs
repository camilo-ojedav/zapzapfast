//! Settings' maintenance block: the chat cleanup and the phone resync.
//!
//! The cleanup lists chats that are only noise: rows with no message at all
//! (often left behind by a sync that never delivered their history), chats
//! whose person is unknown, and ids WhatsApp cannot deliver to. Each row
//! shows its picture, name, number or id and why it is listed; every row is
//! ticked, and the person unticks what they want to keep.

use std::collections::BTreeSet;

use egui::{Rect, Sense, vec2};

use crate::app::App;
use crate::fork::i18n::tr;
use crate::fork::sync::ForkCommand;
use crate::model::Chat;
use crate::theme::{self, Icon};
use crate::ui::widgets;

fn open_id() -> egui::Id {
    egui::Id::new("fork-cleanup-open")
}

fn kept_id() -> egui::Id {
    egui::Id::new("fork-cleanup-kept")
}

/// Why a chat is offered for cleanup, or `None` when it is a real chat.
fn reason(app: &App, chat: &Chat) -> Option<&'static str> {
    if chat.locked {
        return None;
    }
    if !chat.id.contains('@') || chat.id.ends_with("@broadcast") && chat.id != "status@broadcast" {
        return Some("WhatsApp cannot deliver to this id");
    }
    let unknown = app.chat_title(chat) == tr("Unknown");
    match (chat.last.is_none(), unknown) {
        (true, true) => Some("Unknown and empty"),
        (true, false) => Some("No messages"),
        (false, true) => Some("Unknown contact"),
        (false, false) => None,
    }
}

/// The block drawn at the end of Settings.
pub fn settings_block(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    widgets::setting_row(
        ui,
        &palette,
        "Clean up chats",
        "Finds empty chats, unknown contacts and ids that cannot receive messages, and lets you pick which to delete.",
        |ui| {
            if theme::soft_button(ui, &palette, Some(Icon::Trash), "Review", false).clicked() {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(open_id(), true);
                    data.remove::<BTreeSet<String>>(kept_id());
                });
            }
        },
    );
    ui.add_space(8.0);
    widgets::setting_row(
        ui,
        &palette,
        "Sync with the phone",
        "Replays labels and archived chats from your phone, for when they do not match it.",
        |ui| {
            if theme::soft_button(ui, &palette, Some(Icon::Refresh), "Sync", false).clicked() {
                app.backend
                    .send(crate::backend::Command::Fork(ForkCommand::ResyncPhone));
            }
        },
    );
}

/// The cleanup popup, when open.
pub fn window(ctx: &egui::Context, app: &mut App) {
    if !ctx
        .data(|data| data.get_temp::<bool>(open_id()))
        .unwrap_or(false)
    {
        return;
    }
    let palette = app.palette;
    let mut kept: BTreeSet<String> = ctx
        .data(|data| data.get_temp(kept_id()))
        .unwrap_or_default();
    let mut found: Vec<(Chat, &'static str, String)> = app
        .chats
        .iter()
        .filter_map(|chat| reason(app, chat).map(|why| (chat.clone(), why, app.chat_title(chat))))
        .collect();
    found.sort_by(|a, b| a.1.cmp(b.1).then(a.2.cmp(&b.2)));
    let mut close = false;
    let mut purge = false;
    egui::Modal::new(egui::Id::new("fork-cleanup"))
        .backdrop_color(palette.shadow)
        .frame(super::select::panel_frame(&palette))
        .show(ctx, |ui| {
            ui.set_width(520.0_f32.min(ctx.content_rect().width() - 64.0));
            theme::text(
                ui,
                tr("Clean up chats"),
                theme::semibold(17.0),
                palette.text,
            );
            ui.add_space(4.0);
            if found.is_empty() {
                theme::text(
                    ui,
                    tr("Nothing to clean up."),
                    theme::regular(13.0),
                    palette.secondary,
                );
            } else {
                ui.horizontal(|ui| {
                    let chosen = found.len() - kept.len().min(found.len());
                    theme::text(
                        ui,
                        tr("{} of {} chosen")
                            .replacen("{}", &chosen.to_string(), 1)
                            .replacen("{}", &found.len().to_string(), 1),
                        theme::regular(13.0),
                        palette.secondary,
                    );
                    if ui.small_button(tr("Select all")).clicked() {
                        kept.clear();
                    }
                    if ui.small_button(tr("Select none")).clicked() {
                        kept = found.iter().map(|(chat, _, _)| chat.id.clone()).collect();
                    }
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 260.0).max(160.0))
                    .show(ui, |ui| {
                        for (chat, why, title) in &found {
                            row(ui, app, chat, why, title, &mut kept);
                        }
                    });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let chosen = found.len().saturating_sub(
                    found
                        .iter()
                        .filter(|(chat, _, _)| kept.contains(&chat.id))
                        .count(),
                );
                if chosen > 0
                    && theme::soft_button(
                        ui,
                        &palette,
                        Some(Icon::Trash),
                        &tr("Delete {}").replace("{}", &chosen.to_string()),
                        false,
                    )
                    .clicked()
                {
                    purge = true;
                }
                if theme::soft_button(ui, &palette, None, "Close", false).clicked() {
                    close = true;
                }
            });
        });
    if purge {
        let chats: Vec<String> = found
            .iter()
            .map(|(chat, _, _)| chat.id.clone())
            .filter(|id| !kept.contains(id))
            .collect();
        app.backend
            .send(crate::backend::Command::Fork(ForkCommand::PurgeChats(
                chats,
            )));
        close = true;
    }
    ctx.data_mut(|data| {
        if close {
            data.remove::<bool>(open_id());
            data.remove::<BTreeSet<String>>(kept_id());
        } else {
            data.insert_temp(kept_id(), kept);
        }
    });
}

fn row(
    ui: &mut egui::Ui,
    app: &mut App,
    chat: &Chat,
    why: &str,
    title: &str,
    kept: &mut BTreeSet<String>,
) {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let mut chosen = !kept.contains(&chat.id);
    if response.clicked() {
        chosen = !chosen;
    }
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
    }
    // Tick box.
    let tick = Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), vec2(18.0, 18.0));
    ui.painter().rect(
        tick,
        4.0,
        if chosen {
            palette.accent
        } else {
            palette.surface
        },
        egui::Stroke::new(
            1.5,
            if chosen {
                palette.accent
            } else {
                palette.outline
            },
        ),
        egui::StrokeKind::Inside,
    );
    if chosen {
        ui.painter().text(
            tick.center(),
            egui::Align2::CENTER_CENTER,
            "\u{2713}",
            theme::semibold(12.0),
            palette.on_accent,
        );
    }
    let avatar = Rect::from_center_size(rect.left_center() + vec2(58.0, 0.0), vec2(40.0, 40.0));
    let picture = app.avatar(&chat.id);
    widgets::paint_avatar(ui, &palette, avatar, title, &chat.id, picture.as_deref());
    let left = avatar.right() + 12.0;
    let detail = match crate::model::phone_of(&chat.id) {
        Some(digits) => crate::util::phone(digits),
        None => chat.id.clone(),
    };
    let name = if chat.name.is_empty() || chat.name == title {
        title.to_owned()
    } else {
        format!("{title} \u{b7} {}", chat.name)
    };
    let painter = ui.painter();
    painter.text(
        egui::pos2(left, rect.top() + 9.0),
        egui::Align2::LEFT_TOP,
        name,
        theme::medium(14.0),
        palette.text,
    );
    painter.text(
        egui::pos2(left, rect.top() + 30.0),
        egui::Align2::LEFT_TOP,
        format!("{detail}  \u{b7}  {}", tr(why)),
        theme::regular(12.0),
        palette.secondary,
    );
    if chosen {
        kept.remove(&chat.id);
    } else {
        kept.insert(chat.id.clone());
    }
}
