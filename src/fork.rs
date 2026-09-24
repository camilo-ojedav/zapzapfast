//! Everything that makes this tree ZapZapFast rather than ZapFast.
//!
//! The fork is rebased onto upstream regularly, so it keeps its own code here
//! and touches upstream files only with one-line hooks that name something in
//! this module. A hook that grows past a line or two is a conflict waiting to
//! happen: move the logic in here and leave the hook a call. `FORK.md` lists
//! every hook, and `scripts/fork-check.sh` verifies them after a rebase.

pub mod accounts;
pub mod profiles;
pub mod rail;

/// Name shown to people: window title, notifications, tray, menus.
pub const DISPLAY_NAME: &str = "ZapZapFast";

/// Name of the platform directories (`ProjectDirs`), the desktop app id and
/// the binary. Different from upstream's so both install side by side and
/// never share a profile, an archive or a session.
pub const APP_ID: &str = "zapzapfast";

/// Keyring service holding the archive key. Changing it strands every
/// existing archive, so it stays what the fork has always used.
pub const KEYRING_SERVICE: &str = "rocks.zapzapfast.ZapZapFast";

/// Windows AppUserModelID for toasts, matched by the installer's shortcut.
pub const WINDOWS_APP_ID: &str = "me.paolino.zapzapfast";

/// GitHub repository update checks and downloads look at.
pub const REPOSITORY: &str = "camilo-ojedav/zapzapfast";

/// Earlier names whose data upstream adopts on first launch. They belong to
/// ZapFast, never to this fork, which must not move them under its own name.
pub const PREVIOUS_NAMES: [&str; 0] = [];

/// Whether to answer, and defer to, ZapFast copies from before the instance
/// lock on their fixed port. Those are upstream's, not ours, and with several
/// accounts one of ours would hold the port and turn the others away.
pub const LEGACY_INSTANCE_PORT: bool = false;

/// Where the in-app update check asks for the newest release.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/camilo-ojedav/zapzapfast/releases/latest";
