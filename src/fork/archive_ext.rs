//! Archive queries the fork needs: WhatsApp Business labels mirrored into the
//! local label tables. Compiled as a child of `archive` (see the `#[path]`
//! hook there) so it can reach the connection without widening upstream's
//! API.

use rusqlite::params;

use super::{Archive, Result};

/// Label ids that came from the phone carry this prefix, so they never clash
/// with the ones made on this computer.
pub const BUSINESS_PREFIX: &str = "wa-";

impl Archive {
    /// Creates, renames or recolours a label synced from the phone. It skips
    /// the local ceiling: the phone decides how many there are.
    pub fn fork_upsert_business_label(
        &self,
        label: &str,
        name: &str,
        color_hex: &str,
        order: i64,
    ) -> Result<()> {
        let id = format!("{BUSINESS_PREFIX}{label}");
        self.connection.execute(
            "INSERT INTO local_labels (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color,
                created_at = excluded.created_at",
            params![id, name.trim(), color_hex, order],
        )?;
        Ok(())
    }

    /// Removes a label deleted on the phone and takes it off its chats.
    pub fn fork_delete_business_label(&self, label: &str) -> Result<()> {
        self.delete_label(&format!("{BUSINESS_PREFIX}{label}"))
            .map(|_| ())
    }

    /// Puts a phone label on a chat or takes it off. An association that
    /// arrives before its label makes a placeholder, renamed when the label
    /// itself syncs.
    pub fn fork_set_business_chat_label(&self, label: &str, chat: &str, on: bool) -> Result<()> {
        let id = format!("{BUSINESS_PREFIX}{label}");
        if on {
            self.connection.execute(
                "INSERT OR IGNORE INTO local_labels (id, name, color, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, format!("#{label}"), super::DEFAULT_COLOR, i64::MAX / 2],
            )?;
            self.connection.execute(
                "INSERT OR IGNORE INTO local_chat_labels (chat, label) VALUES (?1, ?2)",
                params![chat, id],
            )?;
        } else {
            self.connection.execute(
                "DELETE FROM local_chat_labels WHERE chat = ?1 AND label = ?2",
                params![chat, id],
            )?;
        }
        Ok(())
    }

    /// Every phone sticker that has a file, as `(hash, path)`.
    pub fn fork_sticker_paths(&self) -> Result<Vec<(String, std::path::PathBuf)>> {
        let mut statement = self
            .connection
            .prepare("SELECT hash, path FROM stickers WHERE path IS NOT NULL")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                std::path::PathBuf::from(row.get::<_, String>(1)?),
            ))
        })?;
        rows.collect()
    }
}
