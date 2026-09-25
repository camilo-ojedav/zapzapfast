//! Worker side of the fork: WhatsApp Business labels, the phone resync and
//! the chat cleanup. Compiled as a child of `backend::worker` (see the
//! `#[path]` hook there) so it can use the worker's own helpers.

use std::sync::Arc;

use whatsapp_rust::types::events as wa_events;
use whatsapp_rust::waproto::whatsapp::sync_action_value::label_edit_action::ListType;

use super::{Command, Event, Worker};
use crate::fork::sync::{ForkCommand, label_color};

/// Marker file saying the one-time phone resync already ran for an archive.
const RESYNC_MARKER: &str = "fork-phone-resync-v1";
/// Archive meta key saying attachments were moved into per-chat folders.
const MEDIA_MARKER: &str = "fork_media_by_chat_v1";

impl Worker {
    /// Handles the WhatsApp events the fork owns. Returns true when the event
    /// was consumed; anything else goes on to upstream's handler.
    pub(super) fn fork_wa_event(&mut self, event: &Arc<wa_events::Event>) -> bool {
        use wa_events::Event as E;
        match &**event {
            E::LabelEditUpdate(update) => {
                let action = &update.action;
                // Built-in lists (Unread, Groups, Favorites…) are filters the
                // app already has; only labels and custom lists are kept.
                let listed = matches!(
                    action.r#type,
                    None | Some(ListType::NONE)
                        | Some(ListType::CUSTOM)
                        | Some(ListType::PREDEFINED)
                        | Some(ListType::LEAD)
                );
                let result = if action.deleted == Some(true) || !listed {
                    self.archive.fork_delete_business_label(&update.label_id)
                } else {
                    let name = action.name.clone().unwrap_or_default();
                    self.archive.fork_upsert_business_label(
                        &update.label_id,
                        &name,
                        label_color(action.color),
                        i64::from(action.order_index.unwrap_or(0)),
                    )
                };
                if let Err(error) = result {
                    log::warn!("could not store a phone label: {error}");
                }
                self.fork_labels_changed();
                true
            }
            E::LabelAssociationUpdate(update) => {
                let chat = self.canonical(&update.chat_jid);
                self.ensure_chat(&chat, None);
                let on = update.action.labeled.unwrap_or(false);
                if let Err(error) =
                    self.archive
                        .fork_set_business_chat_label(&update.label_id, &chat, on)
                {
                    log::warn!("could not label a chat from the phone: {error}");
                }
                self.emit_chat(&chat);
                true
            }
            E::Connected(_) => {
                self.fork_resync_once();
                false
            }
            _ => false,
        }
    }

    /// Handles `Command::Fork`.
    pub(super) fn fork_command(&mut self, command: ForkCommand) {
        match command {
            ForkCommand::ResyncPhone => self.fork_resync(),
            ForkCommand::Refresh => self.emit_chats(),
            ForkCommand::PurgeChats(chats) => {
                let client = self.client.clone();
                for chat in chats {
                    let through = crate::util::now();
                    if let (Some(client), Some(jid)) = (client.clone(), Self::jid_of(&chat)) {
                        tokio::spawn(async move {
                            if let Err(error) = client
                                .chat_actions()
                                .delete_chat(
                                    &jid,
                                    true,
                                    Some(whatsapp_rust::message_range(through, None, Vec::new())),
                                )
                                .await
                            {
                                log::info!("cleanup: the phone kept a chat: {error}");
                            }
                        });
                    }
                    self.remove_chat(&chat, through, true);
                }
                self.emit(Event::Info("Removed the selected chats".to_owned()));
            }
        }
    }

    /// Moves attachments from the flat media folder into one folder per chat,
    /// with stickers apart from other files, and points the archive at them.
    /// Rows whose file sits in the old flat place, or whose cache moved under
    /// `accounts/principal`, are found by file name. Cheap once done: a row
    /// already in its chat folder is left alone.
    pub(super) fn fork_reorganize_media(&mut self) {
        let dir = self.dirs.media_cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        crate::fork::cache::discard_staging(&dir);
        // Recent phone stickers keep absolute paths, which the move of the
        // unnamed profile's cache left behind.
        let stickers = self.dirs.sticker_cache_dir();
        for (hash, path) in self.archive.fork_sticker_paths().unwrap_or_default() {
            if path.exists() {
                continue;
            }
            // What follows the cache's `stickers` folder is kept as it was.
            let parts: Vec<_> = path.components().collect();
            let Some(at) = parts
                .iter()
                .rposition(|part| part.as_os_str() == "stickers")
            else {
                continue;
            };
            let moved = parts[at + 1..]
                .iter()
                .fold(stickers.clone(), |dir, part| dir.join(part));
            if moved.exists() {
                let _ = self.archive.set_sticker_path(&hash, &moved);
            }
        }
        if self.archive.meta(MEDIA_MARKER).ok().flatten().is_some() {
            return;
        }
        let rows = match self.archive.media_paths().and_then(|rows| {
            let mut all: Vec<_> = rows
                .into_iter()
                .map(|(chat, id, path)| (chat, id, None, path))
                .collect();
            all.extend(
                self.archive
                    .carousel_media_paths()?
                    .into_iter()
                    .map(|(chat, id, card, path)| (chat, id, Some(card), path)),
            );
            Ok(all)
        }) {
            Ok(rows) => rows,
            Err(error) => {
                log::warn!("could not list attachments to reorganize: {error}");
                return;
            }
        };
        let mut moved = 0;
        for (chat, id, card, path) in rows {
            let Some(name) = path.file_name() else {
                continue;
            };
            let source = if path.exists() {
                path.clone()
            } else {
                dir.join(name)
            };
            // Only files in the flat cache move; a custom download folder
            // is the person's own.
            if !source.exists() || source.parent() != Some(dir.as_path()) {
                continue;
            }
            let extension = source
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("bin");
            let target = crate::fork::cache::chat_dir(&dir, &chat, extension).join(name);
            if std::fs::rename(&source, &target).is_ok()
                && self
                    .archive
                    .put_media_path_at(&chat, &id, card, Some(&target))
                    .is_ok()
            {
                moved += 1;
            }
        }
        let _ = self.archive.set_meta(MEDIA_MARKER, "done");
        log::info!("attachments: {moved} moved into per-chat folders");
    }

    fn fork_labels_changed(&self) {
        self.emit_labels();
    }

    fn fork_marker(&self) -> std::path::PathBuf {
        self.dirs.archive_db().with_file_name(RESYNC_MARKER)
    }

    /// Labels live in the `regular` collection, which upstream never asks
    /// for again once synced, and archive state may have been missed by an
    /// archive older than the fork's handling. One snapshot per archive
    /// replays both.
    fn fork_resync_once(&mut self) {
        if !self.fork_marker().exists() {
            self.fork_resync();
        }
    }

    fn fork_resync(&mut self) {
        let Some(client) = self.client.clone() else {
            self.emit(Event::Error(
                "Connect to WhatsApp to sync with the phone".to_owned(),
            ));
            return;
        };
        let marker = self.fork_marker();
        let commands = self.commands.clone();
        self.emit(Event::Info(
            "Syncing labels and archived chats with the phone…".to_owned(),
        ));
        tokio::spawn(async move {
            use whatsapp_rust::WAPatchName;
            let collections = vec![WAPatchName::Regular, WAPatchName::RegularLow];
            match client
                .resync_app_state(collections, whatsapp_rust::AppStateResyncMode::Snapshot)
                .await
            {
                Ok(report) if report.all_synced() => {
                    let _ = std::fs::write(&marker, b"done");
                    log::info!("phone labels and archive state replayed");
                }
                Ok(report) => log::warn!(
                    "phone resync incomplete: fatal {:?}, retryable {:?}, skipped {:?}",
                    report.fatal,
                    report.retryable,
                    report.skipped
                ),
                Err(error) => log::warn!("phone resync failed: {error}"),
            }
            // Refreshes the chat list with whatever arrived.
            let _ = commands.send(Command::Fork(ForkCommand::Refresh));
        });
    }
}
