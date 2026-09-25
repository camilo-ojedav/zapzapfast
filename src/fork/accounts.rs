//! Several WhatsApp accounts in one window.
//!
//! Upstream is one account per installation. An account there is already a
//! self-contained unit: one [`App`] owns its directories, its backend, its
//! thread and its runtime. So the window holds several of them rather than an
//! account id being threaded through the codebase.
//!
//! [`Accounts`] stands in for [`App`] in `main.rs`. It dereferences to the
//! account on screen, so upstream's window code keeps working unchanged, and
//! overrides the handful of methods that must reach every account.
//!
//! Two rules keep accounts apart:
//!
//! - An account off screen is marked `window_hidden`, reusing upstream's rule
//!   that a hidden window does not read messages. It keeps receiving messages
//!   and raising notifications, but never clears unread counts or sends read
//!   receipts for messages nobody saw.
//! - Each account keeps its own copy of egui's memory. The accounts take turns
//!   drawing the same views into one context, and egui keeps scroll offsets,
//!   cursors and focus there, keyed by ids that would otherwise be shared.
//!   Swapping the memory on a switch keeps every upstream view untouched,
//!   instead of scoping each of its ids by hand.

use super::i18n::tr;
use crate::app::{App, AppOptions};
use crate::backend::Waker;
use crate::paths::AppDirs;
use crate::settings::Settings;
use crate::single_instance::{self, Guard};

struct Account {
    app: App,
    /// `None` for the unnamed profile, the one the installation always had.
    profile: Option<String>,
    /// egui's memory as this account left it, while another is on screen.
    memory: Option<egui::Memory>,
    /// Keeps a second process off this account's archive. Guards belong to
    /// the process, not the window: a window closed to the tray must not free
    /// them, and `Accounts` outlives every window.
    _guard: Option<Guard>,
}

/// Every account of the window, in rail order, and which one is on screen.
pub struct Accounts {
    accounts: Vec<Account>,
    current: usize,
    /// Rail choice, applied at the start of the next frame, before any widget
    /// has read the memory that belongs to the account leaving.
    pending: Option<usize>,
    /// Whether a window exists. Memory is swapped only in a real one.
    window: bool,
    /// Present in a real run, where the rail can link more accounts.
    link: Option<(Waker, AppDirs)>,
    /// Name typed in the link dialog, and why the last attempt failed.
    adding: Option<String>,
    add_error: Option<String>,
    /// Names, emoji and pictures on the rail, and whether it is folded.
    rail: super::rail::Rail,
    /// Account being customised, its draft look and the emoji field's text.
    editing: Option<Editing>,
}

struct Editing {
    index: usize,
    look: super::rail::Look,
    emoji: String,
    error: Option<String>,
}

impl std::ops::Deref for Accounts {
    type Target = App;

    fn deref(&self) -> &App {
        &self.accounts[self.current].app
    }
}

impl std::ops::DerefMut for Accounts {
    fn deref_mut(&mut self) -> &mut App {
        &mut self.accounts[self.current].app
    }
}

impl Accounts {
    /// Holds `first` alone. Demos and tests stay a single account with no rail.
    pub fn single(first: App) -> Self {
        Self {
            accounts: vec![Account {
                app: first,
                profile: None,
                memory: None,
                _guard: None,
            }],
            current: 0,
            pending: None,
            window: false,
            link: None,
            adding: None,
            add_error: None,
            rail: super::rail::Rail::default(),
            editing: None,
        }
    }

    /// Holds `first`, the unnamed profile, and starts every named profile
    /// found beside it. A demo stays a single account with no rail.
    pub fn new(first: App, waker: &Waker, demo: bool) -> Self {
        let base = first.dirs.clone();
        let mut accounts = Self::single(first);
        if demo {
            return accounts;
        }
        accounts.link = Some((waker.clone(), base.clone()));
        accounts.rail = super::rail::Rail::load(&base.config);
        for name in super::profiles::installed(&base) {
            if let Err(error) = accounts.start(&name) {
                log::warn!("profile `{name}` not loaded: {error}");
            }
        }
        accounts
    }

    /// Starts the named profile `name` and returns its rail index.
    fn start(&mut self, name: &str) -> Result<usize, String> {
        let name = super::profiles::validate(name)?;
        if self
            .accounts
            .iter()
            .any(|a| a.profile.as_deref() == Some(&name))
        {
            return Err(tr("`{}` is already open here").replace("{}", &name));
        }
        let (waker, base) = self
            .link
            .as_ref()
            .ok_or(tr("no accounts can be linked here"))?;
        let dirs = super::profiles::dirs(base, &name);
        let guard = match single_instance::acquire(&dirs.runtime, waker, "show") {
            single_instance::Outcome::Only(guard) => guard,
            _ => return Err(tr("`{}` is already open in another window").replace("{}", &name)),
        };
        dirs.ensure()
            .map_err(|error| format!("{} {error}", tr("could not create its directories:")))?;
        let settings = Settings::load(&dirs.settings_file());
        let mut app = App::new(waker, dirs, settings, AppOptions { tray: false });
        app.set_remote_control(&guard);
        self.accounts.push(Account {
            app,
            profile: Some(name),
            memory: None,
            _guard: Some(guard),
        });
        Ok(self.accounts.len() - 1)
    }

    pub fn attach(&mut self, ctx: &egui::Context) {
        self.window = true;
        for account in &mut self.accounts {
            account.app.attach(ctx);
            // A new window starts every account from its fresh memory.
            account.memory = None;
        }
        self.claim_plugins(ctx);
    }

    pub fn window_gone(&mut self) {
        self.window = false;
        for account in &mut self.accounts {
            account.app.window_gone();
            account.memory = None;
        }
    }

    pub fn save_state(&mut self) {
        for account in &mut self.accounts {
            account.app.save_state();
        }
    }

    pub fn shutdown(&mut self) {
        for account in &mut self.accounts {
            account.app.shutdown();
        }
    }

    /// Only the first account owns the tray, and it decides for the window.
    pub fn hides_to_tray(&self) -> bool {
        self.accounts[0].app.hides_to_tray()
    }

    /// Runs every account, the one on screen last, and gathers what the
    /// others asked of the window into the one `main` reads.
    pub fn background_frame(&mut self, ctx: &egui::Context) {
        super::i18n::show(self.accounts[self.current].app.locale);
        if let Some(next) = self.pending.take() {
            self.switch(ctx, next);
        }
        let current = self.current;
        for (index, account) in self.accounts.iter_mut().enumerate() {
            if index != current {
                account.app.window_hidden = true;
                account.app.background_frame(ctx);
            }
        }
        self.accounts[current].app.background_frame(ctx);

        // A notification clicked for an account off screen shows that account.
        let wanted = (0..self.accounts.len())
            .find(|&index| index != current && self.accounts[index].app.wants_show);
        if let Some(index) = wanted {
            self.accounts[index].app.wants_show = false;
            if self.window {
                self.pending = Some(index);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            } else {
                self.current = index;
                self.accounts[index].app.wants_show = true;
            }
        }
        if self
            .accounts
            .iter()
            .any(|account| account.app.quit_requested)
        {
            self.accounts[self.current].app.quit_requested = true;
        }
    }

    /// Draws the rail, then the account on screen.
    pub fn frame_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        super::i18n::show(self.accounts[self.current].app.locale);
        if self.link.is_some() {
            self.rail(ui);
        }
        self.claim_plugins(&ctx);
        let first_hides = self.hides_to_tray();
        let app = &mut self.accounts[self.current].app;
        app.frame_ui(ui);
        // Accounts without the tray would quit on close; the window follows
        // the first account's choice instead.
        if first_hides
            && !app.quit_requested
            && ctx.input(|input| input.viewport().close_requested())
        {
            app.hide_intent = true;
        }
    }

    /// Hands the window from the account on screen to `next`.
    fn switch(&mut self, ctx: &egui::Context, next: usize) {
        if next == self.current || next >= self.accounts.len() {
            return;
        }
        if self.window {
            let options = ctx.options(Clone::clone);
            let leaving = ctx.memory(Clone::clone);
            let arriving = self.accounts[next]
                .memory
                .take()
                .unwrap_or_else(|| fresh_memory(&leaving));
            self.accounts[self.current].memory = Some(leaving);
            ctx.memory_mut(|memory| {
                *memory = arriving;
                // Style and theme belong to the window, not the account.
                memory.options = options;
            });
            ctx.with_plugin::<egui::text_selection::LabelSelectionState, _>(
                egui::text_selection::LabelSelectionState::clear_selection,
            );
        }
        self.current = next;
        let app = &mut self.accounts[next].app;
        app.window_hidden = false;
        app.focus_composer = true;
        ctx.request_repaint();
    }

    /// Points the context's single copy annotator and selection leash at the
    /// account on screen. egui keeps only the first plugin of a type, and
    /// every account attaches its own buffers to it.
    fn claim_plugins(&self, ctx: &egui::Context) {
        let app = &self.accounts[self.current].app;
        let rows = std::sync::Arc::clone(&app.copy_rows);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("copy-rows"), rows.clone()));
        ctx.with_plugin::<crate::transcript::CopyAnnotator, _>(|plugin| plugin.rows = rows);
        let view = std::sync::Arc::clone(&app.selection_view);
        ctx.with_plugin::<crate::ui::conversation::SelectionLeash, _>(|plugin| plugin.view = view);
    }

    fn rail(&mut self, ui: &mut egui::Ui) {
        if self.rail.hidden {
            self.rail_opener(ui.ctx());
            self.link_dialog(ui.ctx());
            self.edit_dialog(ui.ctx());
            return;
        }
        let mut chosen = None;
        let mut add = false;
        let mut edit = None;
        let mut hide = false;
        egui::Panel::left(egui::Id::new("zapzapfast-account-rail"))
            .exact_size(56.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    for (index, account) in self.accounts.iter().enumerate() {
                        let look = self.rail.look(&key(account));
                        let selected = index == self.current;
                        let response = account_button(ui, account, &look, &self.rail, selected);
                        if response.clicked() {
                            chosen = Some(index);
                        }
                        response.context_menu(|ui| {
                            if ui.button(tr("Edit name and icon…")).clicked() {
                                edit = Some(index);
                                ui.close();
                            }
                        });
                        ui.add_space(6.0);
                    }
                    add = ui
                        .add_sized([40.0, 32.0], egui::Button::new("+"))
                        .on_hover_text(tr("Link another account"))
                        .clicked();
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    hide = ui
                        .add_sized([40.0, 32.0], egui::Button::new("‹"))
                        .on_hover_text(tr("Hide the account rail"))
                        .clicked();
                });
            });
        if let Some(index) = chosen.filter(|&index| index != self.current) {
            self.pending = Some(index);
            ui.ctx().request_repaint();
        }
        if add {
            self.adding = Some(String::new());
            self.add_error = None;
        }
        if hide {
            self.rail.hidden = true;
            self.rail.save();
            ui.ctx().request_repaint();
        }
        if let Some(index) = edit {
            let look = self.rail.look(&key(&self.accounts[index]));
            self.editing = Some(Editing {
                index,
                emoji: look.emoji.clone().unwrap_or_default(),
                look,
                error: None,
            });
        }
        self.link_dialog(ui.ctx());
        self.edit_dialog(ui.ctx());
    }

    /// The floating button that brings a folded rail back, dotted when an
    /// account off screen has unread messages.
    fn rail_opener(&mut self, ctx: &egui::Context) {
        let unread = self
            .accounts
            .iter()
            .enumerate()
            .any(|(index, account)| index != self.current && account.app.unread_total() > 0);
        let mut show = false;
        egui::Area::new(egui::Id::new("zapzapfast-rail-opener"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(8.0, -8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let button = egui::Button::new("›").min_size(egui::vec2(28.0, 32.0));
                let response = ui.add(button).on_hover_text(tr("Show the account rail"));
                if unread {
                    ui.painter().circle_filled(
                        response.rect.right_top(),
                        5.0,
                        ui.visuals().error_fg_color,
                    );
                }
                show = response.clicked();
            });
        if show {
            self.rail.hidden = false;
            self.rail.save();
            ctx.request_repaint();
        }
    }

    fn edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editing) = self.editing.take() else {
            return;
        };
        let Some(account) = self.accounts.get(editing.index) else {
            return;
        };
        let key = key(account);
        let fallback = default_label(account);
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut pick = false;
        egui::Window::new(tr("Edit account"))
            .id(egui::Id::new("zapzapfast-edit-account"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Name"));
                let mut name = editing.look.name.clone().unwrap_or_default();
                ui.add(egui::TextEdit::singleline(&mut name).hint_text(fallback.as_str()));
                editing.look.name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
                ui.add_space(6.0);
                ui.label(tr("Emoji (Win + . opens the emoji picker)"));
                ui.add(egui::TextEdit::singleline(&mut editing.emoji).desired_width(80.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    pick = ui.button(tr("Choose picture…")).clicked();
                    if editing.look.picture.is_some() && ui.button(tr("Remove picture")).clicked() {
                        editing.look.picture = None;
                    }
                });
                if let Some(texture) = editing
                    .look
                    .picture
                    .as_deref()
                    .and_then(|file| self.rail.texture(ctx, file))
                {
                    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(40.0, 40.0)));
                }
                ui.small(tr(
                    "A picture wins over an emoji; with neither, the name's initial shows.",
                ));
                if let Some(error) = &editing.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    save = ui.button(tr("Save")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if pick {
            let chosen = rfd::FileDialog::new()
                .set_title(tr("Choose a picture for this account"))
                .add_filter(tr("Images"), &["png", "jpg", "jpeg", "webp", "gif"])
                .pick_file();
            if let Some(path) = chosen {
                match self.rail.import_picture(&key, &path) {
                    Ok(file) => {
                        editing.look.picture = Some(file);
                        editing.error = None;
                    }
                    Err(error) => {
                        editing.error = Some(format!("{} {error}", tr("could not read it:")));
                    }
                }
            }
        }
        if !open || cancel {
            return;
        }
        if !save {
            self.editing = Some(editing);
            return;
        }
        if !editing.emoji.trim().is_empty() && super::rail::emoji_of(&editing.emoji).is_none() {
            editing.error = Some(tr("that is not an emoji").to_owned());
            self.editing = Some(editing);
            return;
        }
        editing.look.emoji = super::rail::emoji_of(&editing.emoji);
        self.rail.set_look(&key, editing.look);
        ctx.request_repaint();
    }

    fn link_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.adding.take() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(tr("Link another account"))
            .id(egui::Id::new("zapzapfast-link-account"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Name this account. It keeps its own chats and settings."));
                ui.add_space(6.0);
                let field = ui.text_edit_singleline(&mut name);
                field.request_focus();
                if let Some(error) = &self.add_error {
                    ui.add_space(4.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    confirm = ui.button(tr("Link")).clicked()
                        || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if !open || cancel {
            self.add_error = None;
            return;
        }
        if !confirm {
            self.adding = Some(name);
            return;
        }
        match self.start(&name) {
            Ok(index) => {
                self.add_error = None;
                self.accounts[index].app.attach(ctx);
                self.pending = Some(index);
                ctx.request_repaint();
            }
            Err(error) => {
                self.add_error = Some(error);
                self.adding = Some(name);
            }
        }
    }
}

/// Memory for an account shown for the first time: the window's, so the
/// image, animation and emoji caches kept there stay shared, without the
/// widget state that belongs to the account leaving.
fn fresh_memory(leaving: &egui::Memory) -> egui::Memory {
    let mut memory = leaving.clone();
    memory
        .data
        .remove_by_type::<egui::containers::scroll_area::State>();
    memory
        .data
        .remove_by_type::<egui::text_edit::TextEditState>();
    memory.data.remove_by_type::<egui::Rect>();
    if let Some(focused) = memory.focused() {
        memory.surrender_focus(focused);
    }
    memory
}

/// Key of an account in the rail's file: its profile, empty for the unnamed.
fn key(account: &Account) -> String {
    account.profile.clone().unwrap_or_default()
}

/// One rail entry: the account's picture, emoji or initial, marked when
/// selected and badged with its unread count.
fn account_button(
    ui: &mut egui::Ui,
    account: &Account,
    look: &super::rail::Look,
    rail: &super::rail::Rail,
    selected: bool,
) -> egui::Response {
    let label = look.name.clone().unwrap_or_else(|| default_label(account));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    ui.painter().rect_filled(rect, 12.0, visuals.bg_fill);
    let texture = look
        .picture
        .as_deref()
        .and_then(|file| rail.texture(ui.ctx(), file));
    if let Some(texture) = texture {
        // The selected account keeps a rim of the highlight around it.
        let inner = rect.shrink(if selected { 3.0 } else { 0.0 });
        egui::Image::new(&texture)
            .corner_radius(10.0)
            .paint_at(ui, inner);
    } else if let Some(emoji) = &look.emoji {
        crate::emoji::paint_cluster(ui, emoji, rect.shrink(6.0));
    } else {
        let initial = label
            .chars()
            .next()
            .map(|first| first.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_owned());
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            initial,
            egui::FontId::proportional(18.0),
            visuals.fg_stroke.color,
        );
    }
    let unread = account.app.unread_total();
    if unread > 0 {
        let corner = rect.right_top() + egui::vec2(-4.0, 4.0);
        ui.painter()
            .circle_filled(corner, 8.0, ui.visuals().error_fg_color);
        ui.painter().text(
            corner,
            egui::Align2::CENTER_CENTER,
            if unread > 99 {
                "99+".to_owned()
            } else {
                unread.to_string()
            },
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );
    }
    response.on_hover_text(label)
}

/// The account's own name, then its profile, then a placeholder: a rail drawn
/// before the phone answers still has to say something.
fn default_label(account: &Account) -> String {
    account
        .app
        .me_name
        .clone()
        .or_else(|| account.profile.clone())
        .unwrap_or_else(|| tr("Main account").to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(root: &std::path::Path, name: &str) -> App {
        let dirs = AppDirs::under(&root.join(name));
        let (mut app, _events) = App::headless(dirs, Settings::default());
        crate::demo::populate(&mut app);
        app
    }

    fn frame(accounts: &mut Accounts, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 420.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx().clone();
                accounts.background_frame(&ctx);
                accounts.frame_ui(ui);
            },
        );
        output.textures_delta.clear();
    }

    fn two_accounts() -> (Accounts, egui::Context) {
        let root = std::env::temp_dir().join(format!(
            "zapfast-accounts-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut accounts = Accounts::single(account(&root, "personal"));
        accounts.accounts.push(Account {
            app: account(&root, "work"),
            profile: Some("work".into()),
            memory: None,
            _guard: None,
        });
        let ctx = egui::Context::default();
        accounts.attach(&ctx);
        (accounts, ctx)
    }

    fn show(accounts: &mut Accounts, ctx: &egui::Context, index: usize) {
        accounts.pending = Some(index);
        for _ in 0..3 {
            frame(accounts, ctx, Vec::new());
        }
    }

    #[test]
    fn the_account_off_screen_counts_as_hidden() {
        let (mut accounts, ctx) = two_accounts();
        frame(&mut accounts, &ctx, Vec::new());
        assert!(!accounts.accounts[0].app.window_hidden);
        assert!(accounts.accounts[1].app.window_hidden);
        show(&mut accounts, &ctx, 1);
        assert!(accounts.accounts[0].app.window_hidden);
        assert!(!accounts.accounts[1].app.window_hidden);
    }

    #[test]
    fn an_account_keeps_its_composer_text_state_across_a_switch() {
        let (mut accounts, ctx) = two_accounts();
        show(&mut accounts, &ctx, 0);
        frame(&mut accounts, &ctx, vec![egui::Event::Text("hola".into())]);
        frame(&mut accounts, &ctx, Vec::new());
        assert_eq!(accounts.accounts[0].app.composer, "hola");
        let with_text = ctx.memory(|m| m.data.count::<egui::text_edit::TextEditState>());

        show(&mut accounts, &ctx, 1);
        assert_eq!(accounts.accounts[1].app.composer, "");
        show(&mut accounts, &ctx, 0);
        assert_eq!(
            ctx.memory(|m| m.data.count::<egui::text_edit::TextEditState>()),
            with_text,
            "the first account came back with its own text-edit state"
        );
    }

    #[test]
    fn quitting_any_account_quits_the_window() {
        let (mut accounts, ctx) = two_accounts();
        frame(&mut accounts, &ctx, Vec::new());
        accounts.accounts[1].app.quit_requested = true;
        frame(&mut accounts, &ctx, Vec::new());
        assert!(accounts.quit_requested);
    }
}
