//! How the fork lays out the cache on disk.
//!
//! ```text
//! cache/accounts/<account>/        principal, work, …
//!     avatars/
//!     media/<chat>/archivos/       photos, videos, audio, documents
//!     media/<chat>/stickers/       stickers received in that chat
//!     stickers/                    the phone's recent stickers, by hash
//! ```
//!
//! Upstream keeps the unnamed profile's cache at the root and every file of
//! every chat in one flat `media/`. Only the cache moves: config and the
//! message archive stay where they are, because the archive's keyring key is
//! derived from its path.

use std::path::{Path, PathBuf};

use crate::paths::AppDirs;

/// Folder, under `cache/accounts`, of the unnamed profile's cache.
pub const MAIN_ACCOUNT: &str = "principal";
const ACCOUNTS: &str = "accounts";

/// The unnamed profile's directories with its cache moved under
/// `accounts/principal`, carrying over what the old root held.
pub fn main_account(mut dirs: AppDirs) -> AppDirs {
    let root = dirs.cache.clone();
    let own = root.join(ACCOUNTS).join(MAIN_ACCOUNT);
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.file_name() == ACCOUNTS {
                continue;
            }
            let target = own.join(entry.file_name());
            if target.exists() || std::fs::create_dir_all(&own).is_err() {
                continue;
            }
            if let Err(error) = std::fs::rename(entry.path(), &target) {
                log::warn!(
                    "could not move {:?} into the account cache: {error}",
                    entry.file_name()
                );
            }
        }
    }
    if std::fs::create_dir_all(&own).is_ok() {
        dirs.cache = own;
    }
    dirs
}

/// The cache root shared by every account, given one account's cache.
pub fn root(cache: &Path) -> PathBuf {
    match (cache.parent(), cache.file_name()) {
        (Some(parent), Some(name))
            if name == MAIN_ACCOUNT && parent.file_name().is_some_and(|n| n == ACCOUNTS) =>
        {
            parent.parent().unwrap_or(parent).to_path_buf()
        }
        _ => cache.to_path_buf(),
    }
}

/// A readable folder name for a chat id: the phone number for people, and a
/// kind prefix for everything else.
pub fn chat_folder(chat: &str) -> String {
    let (user, server) = chat.split_once('@').unwrap_or((chat, ""));
    let user: String = user
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let user = if user.is_empty() {
        "chat".to_owned()
    } else {
        user
    };
    match server {
        "s.whatsapp.net" | "c.us" => user,
        "g.us" => format!("grupo-{user}"),
        "lid" => format!("lid-{user}"),
        "newsletter" => format!("canal-{user}"),
        "broadcast" => format!("difusion-{user}"),
        _ => format!("otro-{user}"),
    }
}

/// Whether a file with this extension is a sticker.
fn is_sticker(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("webp") || extension.eq_ignore_ascii_case("tgs")
}

/// Folder inside `dir` for a chat's file with `extension`, created if needed.
/// A `dir` that does not exist is used as it is, flat, so nothing is created
/// in a place the app never made.
pub fn chat_dir(dir: &Path, chat: &str, extension: &str) -> PathBuf {
    if !dir.is_dir() {
        return dir.to_path_buf();
    }
    let kind = if is_sticker(extension) {
        "stickers"
    } else {
        "archivos"
    };
    let target = dir.join(chat_folder(chat)).join(kind);
    if let Err(error) = std::fs::create_dir_all(&target) {
        log::warn!("could not create a chat media folder: {error}");
        return dir.to_path_buf();
    }
    target
}

/// Removes interrupted downloads left in the chat folders.
pub fn discard_staging(dir: &Path) {
    let Ok(chats) = std::fs::read_dir(dir) else {
        return;
    };
    for chat in chats.flatten().filter(|entry| entry.path().is_dir()) {
        for kind in ["archivos", "stickers"] {
            let Ok(files) = std::fs::read_dir(chat.path().join(kind)) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') && name.ends_with(".part") {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_folders_are_readable() {
        assert_eq!(chat_folder("56912345678@s.whatsapp.net"), "56912345678");
        assert_eq!(chat_folder("120363@g.us"), "grupo-120363");
        assert_eq!(chat_folder("42@lid"), "lid-42");
        assert_eq!(chat_folder("../x@newsletter"), "canal-___x");
    }

    #[test]
    fn stickers_and_files_go_apart() {
        let dir = std::env::temp_dir().join(format!("zzf-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(chat_dir(&dir, "1@g.us", "webp").ends_with("grupo-1/stickers"));
        assert!(chat_dir(&dir, "1@g.us", "jpg").ends_with("grupo-1/archivos"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_root_is_found_from_the_main_account() {
        let root_dir = Path::new("c").join("cache");
        let main = root_dir.join(ACCOUNTS).join(MAIN_ACCOUNT);
        assert_eq!(root(&main), root_dir);
        assert_eq!(root(&root_dir), root_dir);
    }
}
