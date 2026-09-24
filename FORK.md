# ZapZapFast fork guide

ZapZapFast is Camilo's fork of [ZapFast](https://github.com/crmne/zapfast), at
<https://github.com/camilo-ojedav/zapzapfast>. It exists for one reason: **it
holds more than one WhatsApp account in one window**, which upstream declines
("no second account system" in `AGENTS.md`; that rule does not apply here).
Changes are not sent upstream. Do not offer to open a pull request there.

It installs beside ZapFast instead of replacing it: its own binary
(`zapzapfast`), platform directories, keyring service, desktop id, Windows
AppUserModelID, installer AppId and autostart entry. Both run at once.

## The rule that keeps rebases cheap

The fork is rebased onto `upstream/main` regularly (the `zapzapfast-update`
skill runs it). Every line changed in an upstream file is a line that can
conflict, so:

1. **Fork logic lives in fork files**: `src/fork.rs`, `src/fork/*`,
   `FORK.md`, `scripts/fork-*`, and new packaging files named `zapzapfast*`.
   Upstream never touches them, so they never conflict.
2. **Upstream files get hooks, not logic.** A hook is one or two lines that
   name something in `crate::fork`: a constant, a type or a call. A hook that
   grows past that is a conflict waiting to happen: move the code into
   `src/fork/` and leave the hook a call.
3. **Never rename or delete an upstream file.** Renames became modify/delete
   conflicts in the first fork. Add a new file beside it instead.
4. **Never reformat or sweep-rename upstream code.** The crate stays named
   `zapfast` inside (`zapfast::…`, log target `zapfast`); only the package and
   binary are `zapzapfast`. The first fork renamed every `zapfast` token in 55
   files and conflicted in 22 of them on the next sync.
5. **Every hook is listed below and in `scripts/fork-hooks.txt`.** Adding a
   hook means adding it to both, in the same commit.
6. **Commits are grouped by topic**, so a rebase replays a few small commits:
   `fork: identity`, `fork: several accounts`, `fork: packaging`,
   `fork: docs and sync tooling`. Later fork work goes in new commits with the
   `fork:` prefix, one topic each.

## Hooks in upstream files

| File | Hook | Why |
|---|---|---|
| `Cargo.toml` | package and `[[bin]]` name `zapzapfast`, homepage, repository | separate package and binary; `[lib]` stays `zapfast` |
| `build.rs` | `ProductName`/`FileDescription` | Windows file properties |
| `src/lib.rs` | `pub mod fork;` | the fork module |
| `src/main.rs` | `Accounts::new(app, &waker, demo)` wraps the app; `Shell.app`/`slot` hold `Accounts`; `hides_to_tray` closure; window title, app id and eframe name from `fork::` | several accounts, identity |
| `src/paths.rs` | `Self::of(crate::fork::APP_ID)`; `crate::fork::PREVIOUS_NAMES` | own directories; never adopt ZapFast's old data |
| `src/archive/encryption.rs` | `crate::fork::KEYRING_SERVICE` | own archive key; **never change the value** |
| `src/single_instance.rs` | `crate::fork::LEGACY_INSTANCE_PORT &&` before `legacy_instance_answers`, and around `listen_legacy` | the fixed legacy port is upstream's; with it a second account would be turned away |
| `src/updates.rs`, `src/updates/transfer.rs` | `crate::fork::LATEST_RELEASE_URL`, `crate::fork::REPOSITORY` | update checks look at this fork |
| `src/autostart.rs` | `LABEL`, `VALUE` from `fork::` | own login item, never ZapFast's |
| `src/notify.rs`, `src/notify/windows.rs`, `src/notify/badge.rs` | app name, icon, AppUserModelID, desktop id | notifications and badge say ZapZapFast |
| `src/tray.rs`, `src/tray_native.rs` | tray id, title, tooltip | |
| `src/backend/worker.rs` | `.with_os(crate::fork::DISPLAY_NAME)` | the phone lists the linked device as ZapZapFast |
| `packaging/windows/zapfast.iss` | AppName, AppExeName, AppId, output name, AppUserModelID | installs beside ZapFast |
| `.github/workflows/release.yml` | `zapzapfast` binary paths | the binary was renamed |
| `AGENTS.md` | one line at the top pointing here | agents read this first |

## How several accounts work

See the module comment in `src/fork/accounts.rs`. In short:

- `Accounts` owns every account's `App` and dereferences to the one on screen,
  so upstream's `main.rs` drives it as if it were one `App`. It overrides only
  what must reach every account: `attach`, `background_frame`, `frame_ui`,
  `window_gone`, `save_state`, `shutdown`, `hides_to_tray`.
- An account off screen is marked `window_hidden`: it keeps receiving messages
  and raising notifications, but never marks chats read. **Do not break this.**
- Each account keeps its own copy of egui's memory, swapped at the start of
  the frame that switches accounts. Upstream views need no per-account ids.
  An account shown for the first time starts from the window's memory with
  scroll, text-edit and focus state removed, so the image, animation and emoji
  caches kept there stay shared.
- The unnamed profile keeps its files at the root of the platform directories
  (its keyring key derives from the archive's path). Named profiles live under
  `accounts/<name>` in config, state, cache and runtime, and are found by
  listing `config/accounts/`. Each holds its own single-instance lock.
- Only the first account has a tray icon, and it decides whether closing the
  window hides it.

## Data compatibility

These values match the first fork, so existing installs keep their data:
directories `me/paolino/zapzapfast`, keyring service
`rocks.zapzapfast.ZapZapFast`, profiles under `accounts/<name>`, Windows
AppUserModelID `me.paolino.zapzapfast`, installer AppId
`{D43C6357-0C3A-4C48-8A47-FAA2C1B26EF3}`.

## Known trade-offs of the small-diff design

- Interface text that upstream writes as "ZapFast" (About, login screen,
  translations) still says ZapFast. Changing it means touching strings
  upstream edits often.
- macOS identifiers (`me.paolino.fastsapp` bundle ids), Flatpak, Homebrew and
  Omarchy theme files are upstream's. The fork does not ship for those.
- Updates are signed with upstream's key (`src/updates/signing.rs`), so the
  in-app updater refuses unsigned fork releases. Update by building or with
  the installer.
- `--profile <name>` from the first fork is gone; accounts are added from the
  rail's `+` button.

## Wanted next

- Notifications from a background account do not say which account they came
  from.
- Settings and themes are per account; the theme follows the first account.

## Working rules

- Commit messages carry no `Co-Authored-By` line and no mention of the tool
  that wrote them.
- Do not commit work Camilo has not tried yet. Build it, let him run it, and
  commit once he says so.
- Upstream's definition of done in `AGENTS.md` still applies.
- Use `-j 4` for `cargo test --all-targets`; the default job count exhausts
  memory on Camilo's machines.
