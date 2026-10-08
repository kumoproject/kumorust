//! Root node: routing plus message dispatch between feature slices.
//!
//! This is the top of the MVU tree. It owns the root model (route, pane,
//! shared notice) and composes the autonomous `library` and `settings`
//! slices: their messages arrive nested inside [`AppMessage`] and are routed
//! to each slice's own pure reducer. Side effects requested by any reducer are
//! collected into [`AppEffect`] and executed by [`perform`].

use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use windows::core::{Error, HRESULT};
use windows_notifyicon::{NotifyIcon, NotifyIconEvent};
use windows_reactor::*;

use crate::core::config;
use crate::core::i18n::{fmt1, fmt2, tr};
use crate::domain::folder;
use crate::features::library::{self, LibraryMessage, LibraryModel};
use crate::features::settings::{self, SettingsMessage, SettingsModel, UpdateStatus};
use crate::platform::window;
use crate::services::{api, application_update, fingerprint, scanner};

const TRAY_TOOLTIP: &str = "KumoRust";
const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/app.ico");

/// Owns the application-lifetime objects that outlive the main component.
///
/// The notification icon is created on the Reactor UI thread and kept alive by
/// the `App::run_with` resource. The main component can therefore be closed
/// and opened again without recreating the tray integration.
pub(crate) struct AppState {
    app: AppContext,
    icon: RefCell<Option<NotifyIcon>>,
    app_icon: RefCell<Option<AppIcon>>,
    window: RefCell<OpenWindow>,
    activation_listener: RefCell<Option<window::ActivationListener>>,
}

struct AppIcon {
    path: PathBuf,
    path_string: &'static str,
}

enum OpenWindow {
    Closed,
    Opening,
    Open(Callback<()>),
}

#[derive(Clone)]
pub struct KumoAppInput(Rc<AppState>);

impl PartialEq for KumoAppInput {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl AppState {
    pub(crate) fn new(app: AppContext) -> Rc<Self> {
        Rc::new(Self {
            app,
            icon: RefCell::new(None),
            app_icon: RefCell::new(None),
            window: RefCell::new(OpenWindow::Closed),
            activation_listener: RefCell::new(None),
        })
    }

    pub(crate) fn start_activation_listener(self: &Rc<Self>) -> windows::core::Result<()> {
        let events = Rc::downgrade(self);
        let callback = self.app.callback(move || {
            let Some(state) = events.upgrade() else {
                return Ok(());
            };
            state.open_window()
        });
        let listener = window::ActivationListener::start(callback)?;
        *self.activation_listener.borrow_mut() = Some(listener);
        Ok(())
    }

    pub(crate) fn add_icon(self: &Rc<Self>) -> windows_notifyicon::Result<()> {
        let events = Rc::downgrade(self);
        let path = app_icon_path("tray")?;
        let result = NotifyIcon::new(path.clone())
            .tooltip(TRAY_TOOLTIP)
            .on_event(move |event| {
                let Some(state) = events.upgrade() else {
                    return;
                };
                match event {
                    NotifyIconEvent::Activate { .. } => {
                        let _ = window::activate_existing_main_window();
                        if let Err(error) = state.open_window() {
                            eprintln!("could not open KumoRust window: {error}");
                        }
                    }
                    NotifyIconEvent::ContextMenu { position } => state.show_menu(position),
                    NotifyIconEvent::Unavailable => {
                        eprintln!("the Windows Shell could not restore the notification icon");
                        state.exit();
                    }
                    _ => {}
                }
            })
            .build();
        let _ = std::fs::remove_file(path);
        *self.icon.borrow_mut() = Some(result?);
        Ok(())
    }

    fn ensure_app_icon(&self) -> windows_notifyicon::Result<()> {
        if self.app_icon.borrow().is_none() {
            *self.app_icon.borrow_mut() = Some(AppIcon::new()?);
        }
        Ok(())
    }

    fn app_icon_path(&self) -> &'static str {
        self.app_icon
            .borrow()
            .as_ref()
            .expect("application icon should be prepared")
            .path_string
    }

    fn release_app_icon(&self) {
        self.app_icon.borrow_mut().take();
    }

    fn show_menu(self: &Rc<Self>, position: windows_notifyicon::Point) {
        let state = Rc::clone(self);
        let open = Key::from("open");
        let exit = Key::from("exit");
        let menu = Menu::new(
            [
                MenuItem::item(open.clone(), tr("tray.open")),
                MenuItem::item(exit.clone(), tr("tray.exit")),
            ],
            move |key| {
                if key == open {
                    if let Err(error) = state.open_window() {
                        eprintln!("could not open KumoRust window: {error}");
                    }
                } else if key == exit {
                    state.exit();
                }
            },
        );
        if let Err(error) = self
            .app
            .show_menu_at(ScreenPoint::new(position.x, position.y), menu)
        {
            eprintln!("could not show notification icon menu: {error}");
        }
    }

    pub(crate) fn open_window(self: &Rc<Self>) -> windows::core::Result<()> {
        if matches!(&*self.window.borrow(), OpenWindow::Closed) {
            self.ensure_app_icon()?;
        }
        let activate = {
            let mut window = self.window.borrow_mut();
            match &*window {
                OpenWindow::Closed => {
                    *window = OpenWindow::Opening;
                    None
                }
                OpenWindow::Opening => return Ok(()),
                OpenWindow::Open(activate) => Some(activate.clone()),
            }
        };
        if let Some(activate) = activate {
            activate.call(());
            return Ok(());
        }

        if let Err(error) = self
            .app
            .open_component_window::<KumoApp>(KumoAppInput(Rc::clone(self)))
        {
            *self.window.borrow_mut() = OpenWindow::Closed;
            self.release_app_icon();
            return Err(error);
        }
        Ok(())
    }

    fn exit(&self) {
        if let Err(error) = self.app.exit() {
            eprintln!("could not exit KumoRust: {error}");
        }
    }
}

impl AppIcon {
    fn new() -> windows_notifyicon::Result<Self> {
        let path = app_icon_path("window")?;
        // WindowVisuals stores the path as a static string; keep this small string alive for the
        // duration of the process while AppIcon owns the file itself.
        let path_string = Box::leak(path.to_string_lossy().into_owned().into_boxed_str());
        Ok(Self { path, path_string })
    }
}

impl Drop for AppIcon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn app_icon_path(kind: &str) -> windows_notifyicon::Result<PathBuf> {
    // Both the window and notification icon APIs accept a path, while the application icon is
    // embedded in the binary.
    let path = std::env::temp_dir().join(format!("KumoRust-{kind}-{}.ico", std::process::id()));
    std::fs::write(&path, APP_ICON_BYTES).map_err(|error| {
        Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            format!("could not prepare application icon: {error}"),
        )
    })?;
    Ok(path)
}

/// Top-level navigation route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Library,
    Settings,
}

impl Route {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Settings => "settings",
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "settings" => Self::Settings,
            _ => Self::Library,
        }
    }
}

/// Root model: routing plus the sub-models of each feature slice.
#[derive(Clone, Debug, PartialEq)]
pub struct AppModel {
    pub route: Route,
    pub pane_open: bool,
    pub notice: String,
    pub library: LibraryModel,
    pub settings: SettingsModel,
}

impl AppModel {
    pub fn new() -> Self {
        Self {
            route: Route::Library,
            pane_open: true,
            notice: String::new(),
            library: LibraryModel::new(),
            settings: SettingsModel::new(config::load_library_folders()),
        }
    }
}

/// Root message. Feature slices stay autonomous: their messages arrive
/// wrapped as [`AppMessage::Library`] / [`AppMessage::Settings`] and are
/// forwarded to the matching reducer.
#[derive(Clone, Debug)]
pub enum AppMessage {
    /// Activates the existing main window after an external open request.
    Activate,
    /// Completes the native activation fallback queued for an external request.
    ActivationCompleted,
    /// Releases the temporary icon after the initial window has been mounted.
    WindowMounted,
    /// Switch the navigation pane to another route.
    RouteChanged(Route),
    /// The user picked an item in the navigation pane.
    TagChanged(Option<String>),
    /// The navigation pane was opened or closed.
    PaneOpenChanged(bool),
    /// Shared transient notice shown in the current page's info bar.
    Notice(String),
    /// Completes the app-owned update check and package download.
    UpdateFinished(Result<application_update::ApplicationUpdateOutcome, String>),
    /// A library interaction, forwarded to the library reducer.
    Library(LibraryMessage),
    /// A settings interaction, forwarded to the settings reducer.
    Settings(SettingsMessage),
    /// The asynchronous folder picker finished.
    FolderPicked {
        current_folders: Vec<String>,
        result: Result<Option<PathBuf>, String>,
    },
}

/// Side effects requested by any reducer and executed by [`perform`].
#[derive(Debug)]
pub enum AppEffect {
    None,
    /// Run a background scan of `folders`.
    Scan {
        generation: u64,
        folders: Vec<String>,
    },
    /// Show the system folder picker.
    PickFolder {
        current_folders: Vec<String>,
    },
    /// Handle the result returned by the asynchronous folder picker.
    FolderPicked {
        current_folders: Vec<String>,
        result: Result<Option<PathBuf>, String>,
    },
    /// Persist the folder list (and rescan when requested).
    SaveFolders {
        folders: Vec<String>,
        rescan: bool,
    },
    /// Spawn a game process.
    LaunchGame {
        path: String,
        directory: String,
    },
    /// Check and prepare an application update in the background.
    StartUpdater,
    /// Exit after the offline updater has been started.
    ExitAfterUpdate,
    /// Show the system file picker for one executable.
    PickExecutable,
    /// Hash a selected executable in the background.
    Fingerprint {
        path: String,
    },
    /// Look up an executable fingerprint on the server.
    Lookup {
        path: String,
        fingerprint: kumo_contracts::ExecutableFingerprint,
    },
    /// Ask the server to fetch and cache DLsite metadata.
    SearchDlsite {
        rj_code: String,
    },
    /// Persist manually entered metadata and bind it to a fingerprint.
    Register {
        path: String,
        fingerprint: kumo_contracts::ExecutableFingerprint,
        metadata: kumo_contracts::GameMetadata,
    },
    /// Add a resolved game to the local library and metadata cache.
    CommitGame {
        path: String,
        metadata: kumo_contracts::GameMetadata,
    },
    Notice(String),
}

/// Pure root reducer: routes nested messages to their slice reducers and
/// translates slice effects into root effects.
pub fn update(model: &mut AppModel, message: AppMessage) -> AppEffect {
    match message {
        AppMessage::Activate => AppEffect::None,
        AppMessage::ActivationCompleted => AppEffect::None,
        AppMessage::WindowMounted => AppEffect::None,
        AppMessage::RouteChanged(route) => {
            model.route = route;
            AppEffect::None
        }
        AppMessage::TagChanged(tag) => {
            if let Some(tag) = tag {
                model.route = Route::from_tag(&tag);
            }
            AppEffect::None
        }
        AppMessage::PaneOpenChanged(open) => {
            model.pane_open = open;
            AppEffect::None
        }
        AppMessage::Notice(notice) => {
            model.notice = notice;
            AppEffect::None
        }
        AppMessage::UpdateFinished(result) => match result {
            Ok(application_update::ApplicationUpdateOutcome::Started) => AppEffect::ExitAfterUpdate,
            Ok(application_update::ApplicationUpdateOutcome::NoUpdate) => {
                model.settings.update_status = UpdateStatus::Idle;
                AppEffect::None
            }
            Err(message) => {
                model.settings.update_status = UpdateStatus::Error(message);
                AppEffect::None
            }
        },
        AppMessage::Library(message) => match library::update(&mut model.library, message) {
            library::LibraryEffect::None => AppEffect::None,
            library::LibraryEffect::Scan { generation } => AppEffect::Scan {
                generation,
                folders: model.settings.folders.clone(),
            },
            library::LibraryEffect::Launch { path, directory } => {
                AppEffect::LaunchGame { path, directory }
            }
            library::LibraryEffect::PickExecutable => AppEffect::PickExecutable,
            library::LibraryEffect::Fingerprint { path } => AppEffect::Fingerprint { path },
            library::LibraryEffect::Lookup { path, fingerprint } => {
                AppEffect::Lookup { path, fingerprint }
            }
            library::LibraryEffect::SearchDlsite { rj_code } => AppEffect::SearchDlsite { rj_code },
            library::LibraryEffect::Register {
                path,
                fingerprint,
                metadata,
            } => AppEffect::Register {
                path,
                fingerprint,
                metadata,
            },
            library::LibraryEffect::CommitKnown { path, metadata } => {
                AppEffect::CommitGame { path, metadata }
            }
            library::LibraryEffect::Notice(notice) => AppEffect::Notice(notice),
        },
        AppMessage::Settings(message) => match settings::update(&mut model.settings, message) {
            settings::SettingsEffect::None => AppEffect::None,
            settings::SettingsEffect::PickFolder { current_folders } => {
                AppEffect::PickFolder { current_folders }
            }
            settings::SettingsEffect::SaveFolders { folders, rescan } => {
                AppEffect::SaveFolders { folders, rescan }
            }
            settings::SettingsEffect::StartUpdater => AppEffect::StartUpdater,
        },
        AppMessage::FolderPicked {
            current_folders,
            result,
        } => AppEffect::FolderPicked {
            current_folders,
            result,
        },
    }
}

/// The root MVU component.
///
/// `create` initializes the model without touching indexed folders, `update`
/// reduces messages through the root reducer, and `view` renders the model
/// through the feature views. Library scans start only from an explicit user
/// action or a folder-list change.
pub struct KumoApp {
    model: AppModel,
    state: Rc<AppState>,
    app_icon_path: &'static str,
}

impl Component for KumoApp {
    type Message = AppMessage;
    type Input = KumoAppInput;

    fn create(input: &KumoAppInput, context: &ComponentContext<Self>) -> Self {
        // Tray and duplicate-instance requests enter through the component's own queue.
        *input.0.window.borrow_mut() =
            OpenWindow::Open(context.sender().callback(|()| AppMessage::Activate));
        let app_icon_path = input.0.app_icon_path();
        let _ = context.sender().send(AppMessage::WindowMounted);
        Self {
            model: AppModel::new(),
            state: Rc::clone(&input.0),
            app_icon_path,
        }
    }

    fn update(&mut self, message: AppMessage, context: &ComponentContext<Self>) {
        let activate = matches!(&message, &AppMessage::Activate);
        let mounted = matches!(&message, &AppMessage::WindowMounted);
        let effect = update(&mut self.model, message);
        if mounted {
            self.state.release_app_icon();
        }
        if activate {
            let reactor_accepted = context.activate_window();
            let foreground_accepted = context.run_window(|window_handle| {
                window::activate_window_handle(window_handle.as_raw());
                AppMessage::ActivationCompleted
            });
            if !reactor_accepted && !foreground_accepted {
                eprintln!("could not activate KumoRust window");
            }
        } else {
            perform(effect, context);
        }
    }

    fn view(&self, _input: &KumoAppInput, context: &mut ViewContext<Self>) -> View {
        view(&self.model, self.app_icon_path, context)
    }
}

impl Drop for KumoApp {
    fn drop(&mut self) {
        self.state.release_app_icon();
        *self.state.window.borrow_mut() = OpenWindow::Closed;
        if self.state.icon.borrow().is_none() {
            self.state.exit();
        }
    }
}

/// Pure view: renders the current route through the matching feature view and
/// wires navigation to root messages.
pub fn view(
    model: &AppModel,
    app_icon_path: &'static str,
    context: &mut ViewContext<KumoApp>,
) -> View {
    context.window_title(window::MAIN_WINDOW_TITLE);
    context.window_visuals(
        WindowVisuals::new()
            .backdrop(WindowBackdrop::Mica)
            .icon(app_icon_path)
            .constraints(WindowConstraints {
                min_width: Some(800.0),
                min_height: Some(600.0),
                ..Default::default()
            }),
    );

    let menu_items = [
        ("library", tr("nav.library"), Symbol::Library),
        ("settings", tr("nav.settings"), Symbol::Setting),
    ]
    .into_iter()
    .map(|(tag, label, symbol)| {
        KeyedView::new(
            tag,
            NavigationViewItem::new()
                .tag(tag)
                .is_selected(model.route.tag() == tag)
                .content(label)
                .icon(Icon::from(symbol)),
        )
    });

    let content = match model.route {
        Route::Settings => settings::view(&model.settings, &model.notice, context),
        Route::Library => library::view(
            &model.library,
            &model.notice,
            model.settings.folders.is_empty(),
            context,
        ),
    };

    let navigation = NavigationView::new()
        .pane_display_mode(NavigationViewPaneDisplayMode::Auto)
        .is_pane_toggle_button_visible(false)
        .is_pane_open(model.pane_open)
        .on_is_pane_open_changed(context.callback(AppMessage::PaneOpenChanged))
        .is_settings_visible(false)
        .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
        .on_selected_tag_changed(context.callback(|tag: Option<std::rc::Rc<str>>| {
            AppMessage::TagChanged(tag.map(|value| value.to_string()))
        }))
        .keyed_menu_items(menu_items)
        .content(content);

    let title_bar = TitleBar::new()
        .preferred_height(WindowTitleBarHeight::Standard)
        .height(48.0)
        .title("KumoRust")
        .icon(Icon::image_data(EncodedImage::from_static(APP_ICON_BYTES)))
        .is_pane_toggle_button_visible(true)
        .on_pane_toggle_requested(context.message(AppMessage::PaneOpenChanged(!model.pane_open)));

    Grid::new()
        .rows([GridLength::Auto, GridLength::STAR])
        .children((title_bar.grid_row(0), navigation.grid_row(1)))
        .into()
}

/// Runs an effect against the owning component context. This is the only
/// place where the MVU loop touches the reactor (background tasks) and the OS
/// (dialogs, processes, config I/O).
fn perform<C>(effect: AppEffect, context: &ComponentContext<C>)
where
    C: Component<Message = AppMessage>,
{
    match effect {
        AppEffect::None => {}
        AppEffect::Notice(notice) => {
            let _ = context.sender().send(AppMessage::Notice(notice));
        }
        AppEffect::Scan {
            generation,
            folders,
        } => {
            context.spawn_background(move |_token| scan_task(generation, &folders));
        }
        AppEffect::PickFolder { current_folders } => request_folder(current_folders, context),
        AppEffect::FolderPicked {
            current_folders,
            result,
        } => apply_picked_folder(&current_folders, result, context),
        AppEffect::SaveFolders { folders, rescan } => {
            let sender = context.sender();
            match config::save_library_folders(&folders) {
                Ok(()) => {}
                Err(error) => {
                    let _ = sender.send(AppMessage::Notice(fmt1("error.save_failed", error)));
                }
            }
            if rescan {
                let _ = sender.send(AppMessage::Library(LibraryMessage::Refresh));
            }
        }
        AppEffect::LaunchGame { path, directory } => {
            match std::process::Command::new(&path)
                .current_dir(&directory)
                .spawn()
            {
                Ok(_) => {}
                Err(error) => {
                    let _ = context.sender().send(AppMessage::Notice(fmt2(
                        "error.launch_failed",
                        path,
                        error,
                    )));
                }
            }
        }
        AppEffect::StartUpdater => {
            context.spawn_background(move |_token| {
                AppMessage::UpdateFinished(
                    application_update::check_and_start().map_err(|error| error.to_string()),
                )
            });
        }
        AppEffect::ExitAfterUpdate => std::process::exit(0),
        AppEffect::PickExecutable => request_executable(context),
        AppEffect::Fingerprint { path } => {
            context.spawn_background(move |_token| fingerprint_task(path));
        }
        AppEffect::Lookup { path, fingerprint } => {
            context.spawn_background(move |_token| lookup_task(path, fingerprint));
        }
        AppEffect::SearchDlsite { rj_code } => {
            context.spawn_background(move |_token| search_dlsite_task(rj_code));
        }
        AppEffect::Register {
            path,
            fingerprint,
            metadata,
        } => {
            context.spawn_background(move |_token| register_game_task(path, fingerprint, metadata));
        }
        AppEffect::CommitGame { path, metadata } => {
            let sender = context.sender();
            let result = config::save_game_metadata(&path, &metadata)
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    scanner::game_entry_from_path(Path::new(&path), |_| Some(metadata.clone()))
                        .ok_or_else(|| "选择的 exe 已不存在".to_owned())
                });
            let _ = sender.send(AppMessage::Library(LibraryMessage::GameCommitted {
                result,
            }));
        }
    }
}

/// Background scan: runs on a reactor task thread and commits the result as a
/// library message through the normal message loop.
fn scan_task(generation: u64, folders: &[String]) -> AppMessage {
    let output = scanner::scan_folders(folders, |_, _| {});
    AppMessage::Library(LibraryMessage::ScanFinished {
        generation,
        games: output.games,
        inspected: output.inspected,
    })
}

fn request_folder<C>(current_folders: Vec<String>, context: &ComponentContext<C>)
where
    C: Component<Message = AppMessage>,
{
    let _ = windows_pickers::FolderPicker::new()
        .title(tr("folder_picker.title"))
        .request(context, move |result| AppMessage::FolderPicked {
            current_folders,
            result: result.map_err(|error| error.to_string()),
        });
}

fn request_executable<C>(context: &ComponentContext<C>)
where
    C: Component<Message = AppMessage>,
{
    let _ = windows_pickers::OpenFilePicker::new()
        .title(tr("library.add_game.picker_title"))
        .filter_extensions(tr("library.add_game.exe_filter"), ["exe"])
        .request(context, |result| {
            AppMessage::Library(LibraryMessage::ExecutablePicked {
                result: result.map_err(|error| error.to_string()),
            })
        });
}

fn fingerprint_task(path: String) -> AppMessage {
    let result = fingerprint::fingerprint(Path::new(&path));
    AppMessage::Library(LibraryMessage::FingerprintReady { path, result })
}

fn lookup_task(path: String, fingerprint: kumo_contracts::ExecutableFingerprint) -> AppMessage {
    let result = api::lookup_game(fingerprint.clone());
    AppMessage::Library(LibraryMessage::LookupFinished {
        path,
        fingerprint,
        result,
    })
}

fn search_dlsite_task(rj_code: String) -> AppMessage {
    AppMessage::Library(LibraryMessage::DlsiteSearchFinished(api::search_dlsite(
        rj_code,
    )))
}

fn register_game_task(
    path: String,
    fingerprint: kumo_contracts::ExecutableFingerprint,
    metadata: kumo_contracts::GameMetadata,
) -> AppMessage {
    let result = api::register_game(fingerprint.clone(), metadata);
    AppMessage::Library(LibraryMessage::RegistrationFinished {
        path,
        fingerprint,
        result,
    })
}

fn apply_picked_folder<C>(
    current_folders: &[String],
    result: Result<Option<PathBuf>, String>,
    context: &ComponentContext<C>,
) where
    C: Component<Message = AppMessage>,
{
    let path = match result {
        Ok(Some(path)) => path,
        Ok(None) => return,
        Err(error) => {
            let _ = context.sender().send(AppMessage::Notice(fmt1(
                "error.folder_picker_failed",
                error,
            )));
            return;
        }
    };
    let folder = path.to_string_lossy().into_owned();
    let sender = context.sender();
    if folder::contains_folder(current_folders, &folder) {
        let _ = sender.send(AppMessage::Notice(tr("error.folder_duplicate").to_string()));
        return;
    }

    let mut next = current_folders.to_vec();
    next.push(folder);
    let _ = sender.send(AppMessage::Settings(SettingsMessage::ApplyFolders {
        folders: next,
        rescan: true,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::library::ScanStatus;

    #[test]
    fn new_model_starts_without_a_library_scan() {
        let model = AppModel::new();

        assert!(model.library.games.is_empty());
        assert_eq!(model.library.scan, ScanStatus::Idle);
        assert_eq!(model.library.scan_generation, 0);
    }

    #[test]
    fn manual_refresh_still_starts_a_library_scan() {
        let mut model = AppModel::new();

        let effect = update(&mut model, AppMessage::Library(LibraryMessage::Refresh));

        assert!(matches!(effect, AppEffect::Scan { generation: 1, .. }));
        assert!(matches!(
            model.library.scan,
            ScanStatus::Scanning {
                inspected: 0,
                found: 0
            }
        ));
    }
}
