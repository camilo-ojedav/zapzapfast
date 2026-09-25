//! Where each account keeps its files.
//!
//! The first account is the unnamed profile and keeps its files at the root
//! of the platform directories: its archive key in the keyring is derived from
//! the archive's own directory, so moving those files would strand it. Every
//! other account is a named profile under `accounts/<name>` in each of them.

use crate::paths::AppDirs;

const ACCOUNTS: &str = "accounts";

/// Accepts a plain slug only. A profile name becomes a directory name, so
/// anything carrying a separator, a leading dot, or uppercase could reach
/// outside the data directory or collide with a sibling on a case-insensitive
/// filesystem. Surrounding blanks are a typing accident and are dropped.
pub fn validate(name: &str) -> Result<String, String> {
    let name = name.trim();
    let plain = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if plain {
        Ok(name.to_owned())
    } else {
        Err(crate::fork::i18n::tr(
            "a profile name uses lowercase letters, digits and inner dashes; `{}` does not",
        )
        .replace("{}", name))
    }
}

/// Directories of the named profile `name` inside `base`, the unnamed
/// profile's directories. Built from a copy of `base`, so a directory
/// upstream adds later is placed under the profile without a change here.
pub fn dirs(base: &AppDirs, name: &str) -> AppDirs {
    let mut dirs = base.clone();
    for dir in [
        &mut dirs.config,
        &mut dirs.state,
        &mut dirs.cache,
        &mut dirs.runtime,
    ] {
        *dir = dir.join(ACCOUNTS).join(name);
    }
    dirs
}

/// Named profiles that exist in `base`, sorted.
///
/// The set is the directory listing itself rather than a registry file: a
/// profile exists exactly when its directory does, so adding or removing an
/// account cannot drift out of step with a list of them. Unreadable or oddly
/// named entries are skipped, never guessed at.
pub fn installed(base: &AppDirs) -> Vec<String> {
    let Ok(entries) = base.config.join(ACCOUNTS).read_dir() else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| validate(entry.file_name().to_str()?).ok())
        .collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_name_cannot_leave_its_directory() {
        let long = "a".repeat(33);
        for refused in [
            "../escape",
            "with/separator",
            "Uppercase",
            "",
            "   ",
            "-leading",
            "trailing-",
            long.as_str(),
        ] {
            assert!(validate(refused).is_err(), "`{refused}` was accepted");
        }
        assert_eq!(validate("two-phones").unwrap(), "two-phones");
        assert_eq!(validate("  work  ").unwrap(), "work");
    }

    #[test]
    fn named_profiles_live_under_accounts_and_are_listed() {
        let root = std::env::temp_dir().join(format!("zapfast-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let base = AppDirs::under(&root);
        let work = dirs(&base, "work");
        assert_eq!(work.state, base.state.join("accounts").join("work"));
        assert_eq!(work.runtime, base.runtime.join("accounts").join("work"));
        assert!(installed(&base).is_empty());

        work.ensure().unwrap();
        std::fs::create_dir_all(base.config.join("accounts").join("Bad Name")).unwrap();
        dirs(&base, "home").ensure().unwrap();
        assert_eq!(installed(&base), ["home", "work"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
