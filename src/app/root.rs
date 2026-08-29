use windows_reactor::*;

use crate::app::model::AppState;
use crate::app::{Effect, Msg, Page, scan, store, update};
use crate::domain::settings;
use crate::features::library::library_page;
use crate::features::settings::settings_page;
use crate::platform::{tray, window};

/// The root MVU component.
///
/// It owns the model, reduces messages through the pure reducer and renders
/// the current state through pure view functions. This is the counterpart of
/// the `Component` in the reference architecture: `create` initializes the
/// model, `update` reduces messages, `view` renders the model.
pub struct KumoApp {
    model: AppState,
}

impl Component for KumoApp {
    type Message = Msg;
    type Input = ();

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        tray::ensure_initialized();

        // Bootstrap: scan the configured folders exactly once at startup.
        let mut model = AppState::new(settings::load_library_folders());
        if let Effect::Scan { generation, folders } =
            update::update(&mut model, Msg::RefreshLibrary)
        {
            context.spawn_background(move |_token| scan::scan_message(generation, &folders));
        }
        Self { model }
    }

    fn update(&mut self, message: Msg, context: &ComponentContext<Self>) {
        let effect = update::update(&mut self.model, message);
        store::perform(effect, context);
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        view(&self.model, context)
    }
}

/// Pure view: renders the model and wires every interaction to a message.
pub fn view(model: &AppState, context: &mut ViewContext<KumoApp>) -> View {
    context.window_title(window::MAIN_WINDOW_TITLE);
    context.window_visuals(
        WindowVisuals::new()
            .backdrop(WindowBackdrop::Mica)
            .client_size(1080.0, 720.0),
    );

    let menu_items = [
        ("library", "库", Symbol::Library),
        ("settings", "设置", Symbol::Setting),
    ]
    .into_iter()
    .map(|(tag, label, symbol)| {
        KeyedView::new(
            tag,
            NavigationViewItem::new()
                .tag(tag)
                .is_selected(model.page.tag() == tag)
                .slots([
                    SlotView::new(NavigationViewItemSlot::Content, label),
                    SlotView::new(
                        NavigationViewItemSlot::Icon,
                        SymbolIcon::new().symbol(symbol),
                    ),
                ]),
        )
    });

    let content = if model.page == Page::Settings {
        settings_page(model, context)
    } else {
        library_page(model, context)
    };

    NavigationView::new()
        .pane_display_mode(NavigationViewPaneDisplayMode::Left)
        .is_pane_open(model.pane_open)
        .on_is_pane_open_changed(context.callback(Msg::PaneOpenChanged))
        .is_settings_visible(false)
        .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
        .pane_title("KumoRust")
        .on_selected_tag_changed(context.callback(Msg::NavigateTag))
        .slots([
            SlotView::collection(NavigationViewSlot::MenuItems, menu_items),
            SlotView::new(NavigationViewSlot::Content, content),
        ])
}
