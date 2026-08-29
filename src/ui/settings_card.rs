use windows_reactor::*;

use crate::ui::tokens::TEXT_SECONDARY;

/// The card surface variants: standalone, expander header, expander item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardSurface {
    Default,
    ExpanderHeader,
    ExpanderItem,
}

/// A Windows 11-style settings row.
///
/// This is a declarative Reactor counterpart to the Toolkit `SettingsCard`.
/// It deliberately stays composed of built-in WinUI controls so it works in a
/// Rust-only application without bringing a XAML control assembly into the
/// binary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsCard {
    header: String,
    description: Option<String>,
    header_icon: Option<View>,
    content: Option<View>,
}

impl SettingsCard {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            ..Self::default()
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then_some(description);
        self
    }

    pub fn header_icon(mut self, icon: impl Into<View>) -> Self {
        self.header_icon = Some(icon.into());
        self
    }

    /// Set the control shown in the trailing column of the card.
    pub fn content(mut self, content: impl Into<View>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Render the card with the standard standalone-card spacing.
    pub fn into_element(self) -> View {
        render_card(self, CardSurface::Default)
    }

    /// Render the card as an item inside a `SettingsExpander`.
    pub fn into_expander_item(self) -> View {
        render_card(self, CardSurface::ExpanderItem)
    }
}

impl From<SettingsCard> for View {
    fn from(card: SettingsCard) -> Self {
        card.into_element()
    }
}

/// A collapsible group of `SettingsCard` items.
///
/// The expand/collapse behavior is delegated to Reactor's built-in
/// `Expander`, while the header and item surface follow the Toolkit layout:
/// one settings-card header, a compact list of cards, and an optional footer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsExpander {
    header: String,
    description: Option<String>,
    header_icon: Option<View>,
    content: Option<View>,
    items: Vec<View>,
    items_footer: Option<View>,
    is_expanded: bool,
    on_expanding: Option<Callback<bool>>,
}

impl SettingsExpander {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            ..Self::default()
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then_some(description);
        self
    }

    pub fn header_icon(mut self, icon: impl Into<View>) -> Self {
        self.header_icon = Some(icon.into());
        self
    }

    /// Set the optional setting control shown in the header's trailing column.
    pub fn content(mut self, content: impl Into<View>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn items(mut self, items: Vec<View>) -> Self {
        self.items = items;
        self
    }

    pub fn items_footer(mut self, content: impl Into<View>) -> Self {
        self.items_footer = Some(content.into());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    /// Alias matching the Toolkit property name.
    #[allow(clippy::wrong_self_convention)]
    pub fn is_expanded(self, expanded: bool) -> Self {
        self.expanded(expanded)
    }

    /// Report user expand/collapse interactions so the model stays in control.
    pub fn on_expanding(mut self, callback: impl IntoPayloadCallback<bool>) -> Self {
        self.on_expanding = Some(callback.into_payload_callback());
        self
    }

    pub fn into_element(self) -> View {
        let header_card = render_card(
            SettingsCard {
                header: self.header,
                description: self.description,
                header_icon: self.header_icon,
                content: self.content,
            },
            CardSurface::ExpanderHeader,
        );

        let mut item_views = Vec::with_capacity(
            self.items.len() + usize::from(self.items_footer.is_some()),
        );
        item_views.extend(self.items);
        if let Some(footer) = self.items_footer {
            item_views.push(footer);
        }
        let items = StackPanel::new().keyed_children(
            item_views
                .into_iter()
                .enumerate()
                .map(|(index, view)| KeyedView::new(index, view)),
        );

        let mut expander = Expander::new().is_expanded(self.is_expanded);
        if let Some(callback) = self.on_expanding {
            expander = expander.on_is_expanded_changed(callback);
        }
        let expander = expander.slots([
            SlotView::new(ExpanderSlot::Header, header_card),
            SlotView::new(ExpanderSlot::Content, items),
        ]);

        Border::new()
            .min_width(148.0)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .content(expander)
    }
}

impl From<SettingsExpander> for View {
    fn from(expander: SettingsExpander) -> Self {
        expander.into_element()
    }
}

fn render_card(card: SettingsCard, surface: CardSurface) -> View {
    let SettingsCard {
        header,
        description,
        header_icon,
        content,
    } = card;

    let has_header = !header.is_empty() || description.is_some() || header_icon.is_some();

    let icon = match header_icon {
        Some(icon) => Border::new()
            .width(20.0)
            .height(20.0)
            .margin(Thickness::new(2.0, 0.0, 20.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(0)
            .content(icon),
        None => Border::new()
            .width(0.0)
            .height(1.0)
            .grid_column(0)
            .content(View::empty()),
    };
    let description_view: View = match description {
        Some(description) => TextBlock::new()
            .text(description)
            .font_size(12.0)
            .foreground(TEXT_SECONDARY)
            .text_wrapping(TextWrapping::Wrap)
            .max_lines(3)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .into(),
        None => View::empty(),
    };
    let details = Border::new()
        .vertical_alignment(VerticalAlignment::Center)
        .grid_column(1)
        .content(StackPanel::new().spacing(4.0).children((
            TextBlock::new()
                .text(header.clone())
                .font_size(14.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .max_lines(1)
                .text_trimming(TextTrimming::CharacterEllipsis),
            description_view,
        )));
    let content_view = content.unwrap_or_else(View::empty);

    let layout: View = if has_header {
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                icon,
                details,
                Border::new()
                    .grid_column(2)
                    .horizontal_alignment(HorizontalAlignment::Right)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(content_view),
            ))
    } else {
        content_view
    };

    let (padding, min_height, min_width, background, border_brush, border_thickness, radius) =
        match surface {
            CardSurface::Default => (
                Thickness::uniform(16.0),
                68.0,
                148.0,
                Some(ThemeBrush::CardBackground),
                Some(ThemeBrush::CardStroke),
                Some(Thickness::uniform(1.0)),
                8.0,
            ),
            CardSurface::ExpanderHeader => (
                Thickness::new(16.0, 16.0, 4.0, 16.0),
                68.0,
                0.0,
                None,
                None,
                None,
                0.0,
            ),
            CardSurface::ExpanderItem => (
                Thickness::new(58.0, 8.0, 44.0, 8.0),
                52.0,
                0.0,
                Some(ThemeBrush::CardBackground),
                Some(ThemeBrush::CardStroke),
                Some(Thickness::new(0.0, 1.0, 0.0, 0.0)),
                0.0,
            ),
        };

    let mut card_view = Border::new()
        .min_width(min_width)
        .min_height(min_height)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .padding(padding)
        .corner_radius(radius);
    if let Some(background) = background {
        card_view = card_view.background(background);
    }
    if let Some(border_brush) = border_brush {
        card_view = card_view.border_brush(border_brush);
    }
    if let Some(border_thickness) = border_thickness {
        card_view = card_view.border_thickness(border_thickness);
    }
    card_view.content(layout)
}
