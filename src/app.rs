//! Root node: routing plus message dispatch between feature slices.
//!
//! This is the top of the MVU tree. It owns the root model (route, pane,
//! shared notice) and composes the autonomous `library` and `settings`
//! slices: their messages arrive nested inside [`AppMessage`] and are routed
//! to each slice's own pure reducer. Side effects requested by any reducer are
//! collected into [`AppEffect`] and executed by [`perform`].

use windows_reactor::*;

use crate::core::config;
use crate::core::i18n::{fmt1, fmt2, tr};
use crate::domain::folder;
use crate::features::library::{self, LibraryMessage, LibraryModel};
use crate::features::settings::{self, SettingsMessage, SettingsModel};
use crate::platform::{tray, window};
use crate::services::{application_update, scanner, updater};

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
    /// Switch the navigation pane to another route.
    RouteChanged(Route),
    /// The user picked an item in the navigation pane.
    TagChanged(Option<String>),
    /// The navigation pane was opened or closed.
    PaneOpenChanged(bool),
    /// Shared transient notice shown in the current page's info bar.
    Notice(String),
    /// A library interaction, forwarded to the library reducer.
    Library(LibraryMessage),
    /// A settings interaction, forwarded to the settings reducer.
    Settings(SettingsMessage),
    /// The main process finished checking and preparing an application update.
    ApplicationUpdatePrepared(Result<application_update::ApplicationUpdatePreparation, String>),
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
    /// Download and prepare an application update in the main process.
    PrepareApplicationUpdate,
    /// Launch the standalone updater for a prepared package.
    ApplyPreparedUpdate(std::path::PathBuf),
}

/// Pure root reducer: routes nested messages to their slice reducers and
/// translates slice effects into root effects.
pub fn update(model: &mut AppModel, message: AppMessage) -> AppEffect {
    match message {
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
        AppMessage::Library(message) => match library::update(&mut model.library, message) {
            library::LibraryEffect::None => AppEffect::None,
            library::LibraryEffect::Scan { generation } => AppEffect::Scan {
                generation,
                folders: model.settings.folders.clone(),
            },
            library::LibraryEffect::Launch { path, directory } => {
                AppEffect::LaunchGame { path, directory }
            }
        },
        AppMessage::Settings(message) => match settings::update(&mut model.settings, message) {
            settings::SettingsEffect::None => AppEffect::None,
            settings::SettingsEffect::PickFolder { current_folders } => {
                AppEffect::PickFolder { current_folders }
            }
            settings::SettingsEffect::SaveFolders { folders, rescan } => {
                AppEffect::SaveFolders { folders, rescan }
            }
            settings::SettingsEffect::PrepareApplicationUpdate => {
                AppEffect::PrepareApplicationUpdate
            }
            settings::SettingsEffect::ApplyPreparedUpdate(package_directory) => {
                AppEffect::ApplyPreparedUpdate(package_directory)
            }
        },
        AppMessage::ApplicationUpdatePrepared(result) => match result {
            Ok(application_update::ApplicationUpdatePreparation::NoUpdate) => {
                settings::update(&mut model.settings, SettingsMessage::UpdateFinished);
                AppEffect::None
            }
            Ok(application_update::ApplicationUpdatePreparation::Ready(package_directory)) => {
                match settings::update(
                    &mut model.settings,
                    SettingsMessage::UpdateReady(package_directory),
                ) {
                    settings::SettingsEffect::ApplyPreparedUpdate(package_directory) => {
                        AppEffect::ApplyPreparedUpdate(package_directory)
                    }
                    _ => AppEffect::None,
                }
            }
            Err(error) => {
                settings::update(&mut model.settings, SettingsMessage::UpdateFailed(error));
                AppEffect::None
            }
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
}

impl Component for KumoApp {
    type Message = AppMessage;
    type Input = ();

    fn create(_input: &(), _context: &ComponentContext<Self>) -> Self {
        tray::ensure_initialized();
        Self {
            model: AppModel::new(),
        }
    }

    fn update(&mut self, message: AppMessage, context: &ComponentContext<Self>) {
        let effect = update(&mut self.model, message);
        perform(effect, context);
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        view(&self.model, context)
    }
}

/// Pure view: renders the current route through the matching feature view and
/// wires navigation to root messages.
pub fn view(model: &AppModel, context: &mut ViewContext<KumoApp>) -> View {
    context.window_title(window::MAIN_WINDOW_TITLE);
    context.use_effect("main-window-close-to-hide", (), || {
        window::install_close_to_hide();
        None
    });
    context.window_visuals(
        WindowVisuals::new()
            .backdrop(WindowBackdrop::Mica)
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
                .slots([
                    SlotView::new(NavigationViewItemSlot::Content, label),
                    SlotView::new(
                        NavigationViewItemSlot::Icon,
                        SymbolIcon::new().symbol(symbol),
                    ),
                ]),
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
        .on_selected_tag_changed(context.callback(AppMessage::TagChanged))
        .slots([
            SlotView::collection(NavigationViewSlot::MenuItems, menu_items),
            SlotView::new(NavigationViewSlot::Content, content),
        ]);

    let title_bar = TitleBar::new()
        .preferred_height(WindowTitleBarHeight::Standard)
        .height(48.0)
        .title("KumoRust")
        .is_pane_toggle_button_visible(true)
        .on_pane_toggle_requested(context.message(AppMessage::PaneOpenChanged(!model.pane_open)));

    StackPanel::new()
        .orientation(Orientation::Vertical)
        .children((title_bar, navigation))
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
        AppEffect::Scan {
            generation,
            folders,
        } => {
            context.spawn_background(move |_token| scan_task(generation, &folders));
        }
        AppEffect::PickFolder { current_folders } => pick_folder(&current_folders, context),
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
        AppEffect::PrepareApplicationUpdate => {
            context.spawn_background(move |_token| {
                AppMessage::ApplicationUpdatePrepared(
                    application_update::prepare_application_update()
                        .map_err(|error| error.to_string()),
                )
            });
        }
        AppEffect::ApplyPreparedUpdate(package_directory) => {
            match updater::start_prepared_update(package_directory) {
                Ok(()) => std::process::exit(0),
                Err(error) => {
                    let _ =
                        context
                            .sender()
                            .send(AppMessage::Settings(SettingsMessage::UpdateFailed(fmt1(
                                "error.update_failed",
                                error,
                            ))));
                }
            }
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

fn pick_folder<C>(current_folders: &[String], context: &ComponentContext<C>)
where
    C: Component<Message = AppMessage>,
{
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("folder_picker.title"))
        .pick_folder()
    else {
        return;
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
