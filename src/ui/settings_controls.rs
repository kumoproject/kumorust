use windows_reactor::*;

/// Placement of a settings card's trailing content.
///
/// The Windows Community Toolkit uses the same three modes for
/// `SettingsCard.ContentAlignment`: right, left, and vertical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsCardContentAlignment {
    /// Keep the setting control in the trailing column.
    #[default]
    Right,
    /// Show the setting control as the only content in the card.
    Left,
    /// Put the setting control below the heading.
    Vertical,
}

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
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsCard {
    header: String,
    description: Option<String>,
    header_icon: Option<Element>,
    content: Option<Element>,
    content_alignment: SettingsCardContentAlignment,
    is_click_enabled: bool,
    on_click: Option<Callback<()>>,
    action_icon: Option<Element>,
    is_action_icon_visible: bool,
}

impl Default for SettingsCard {
    fn default() -> Self {
        Self {
            header: String::new(),
            description: None,
            header_icon: None,
            content: None,
            content_alignment: SettingsCardContentAlignment::default(),
            is_click_enabled: false,
            on_click: None,
            action_icon: None,
            is_action_icon_visible: true,
        }
    }
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

    pub fn header_icon(mut self, icon: impl Into<Element>) -> Self {
        self.header_icon = Some(icon.into());
        self
    }

    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn content_alignment(mut self, alignment: SettingsCardContentAlignment) -> Self {
        self.content_alignment = alignment;
        self
    }

    pub fn is_click_enabled(mut self, enabled: bool) -> Self {
        self.is_click_enabled = enabled;
        self
    }

    pub fn on_click(mut self, callback: impl IntoUnitCallback) -> Self {
        self.is_click_enabled = true;
        self.on_click = Some(callback.into_unit_callback());
        self
    }

    /// Set the trailing icon shown when the card is clickable.
    pub fn action_icon(mut self, icon: impl Into<Element>) -> Self {
        self.action_icon = Some(icon.into());
        self
    }

    /// Control whether a clickable card shows its trailing action icon.
    pub fn is_action_icon_visible(mut self, visible: bool) -> Self {
        self.is_action_icon_visible = visible;
        self
    }

    /// Render the card with the standard standalone-card spacing.
    pub fn into_element(self) -> Element {
        render_card(self, CardSurface::Default)
    }

    /// Render the card as an item inside a `SettingsExpander`.
    pub fn into_expander_item(self) -> Element {
        render_card(self, CardSurface::ExpanderItem)
    }
}

impl From<SettingsCard> for Element {
    fn from(card: SettingsCard) -> Self {
        card.into_element()
    }
}

/// A collapsible group of `SettingsCard` items.
///
/// The actual expand/collapse behavior is delegated to Reactor's built-in
/// `Expander`, while the header and item surface follow the Toolkit layout:
/// one settings-card header, a compact list of cards, and optional header/footer
/// slots for arbitrary content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsExpander {
    header: String,
    description: Option<String>,
    header_icon: Option<Element>,
    content: Option<Element>,
    items: Vec<Element>,
    items_header: Option<Element>,
    items_footer: Option<Element>,
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

    pub fn header_icon(mut self, icon: impl Into<Element>) -> Self {
        self.header_icon = Some(icon.into());
        self
    }

    /// Set the optional setting control shown in the header's trailing column.
    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn items(mut self, items: Vec<Element>) -> Self {
        self.items = items;
        self
    }

    pub fn items_header(mut self, content: impl Into<Element>) -> Self {
        self.items_header = Some(content.into());
        self
    }

    pub fn items_footer(mut self, content: impl Into<Element>) -> Self {
        self.items_footer = Some(content.into());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    /// Alias matching the Toolkit property name.
    pub fn is_expanded(self, expanded: bool) -> Self {
        self.expanded(expanded)
    }

    pub fn on_expanding(mut self, callback: impl IntoCallback<bool>) -> Self {
        self.on_expanding = Some(callback.into_callback());
        self
    }

    pub fn into_element(self) -> Element {
        let automation_name = if self.header.is_empty() {
            String::from("设置分组")
        } else {
            self.header.clone()
        };
        let header = render_card(
            SettingsCard {
                header: self.header,
                description: self.description,
                header_icon: self.header_icon,
                content: self.content,
                ..SettingsCard::default()
            },
            CardSurface::ExpanderHeader,
        );

        let mut item_children = Vec::with_capacity(
            self.items.len()
                + usize::from(self.items_header.is_some())
                + usize::from(self.items_footer.is_some()),
        );
        if let Some(items_header) = self.items_header {
            item_children.push(items_header);
        }
        item_children.extend(self.items);
        if let Some(items_footer) = self.items_footer {
            item_children.push(items_footer);
        }
        let items = vstack(item_children)
            .width(916.0)
            .horizontal_alignment(HorizontalAlignment::Stretch);

        let mut expander = Expander::new(items)
            .header_content(header)
            .expanded(self.is_expanded)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .padding(0.0)
            .resources([
                // The outer Border owns the card surface. These overrides keep
                // the platform Expander template from painting a second fill.
                ("ExpanderHeaderBackground", Color::transparent()),
                ("ExpanderHeaderBackgroundPointerOver", Color::transparent()),
                ("ExpanderHeaderBackgroundPressed", Color::transparent()),
                ("ExpanderHeaderBackgroundDisabled", Color::transparent()),
                ("ExpanderContentBackground", Color::transparent()),
                ("ExpanderContentBorderBrush", Color::transparent()),
            ]);

        if let Some(callback) = self.on_expanding {
            expander = expander.on_expanding(callback);
        }

        border(expander)
            .min_width(148.0)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .background(tokens::CardBackground)
            .border_brush(tokens::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .automation_name(automation_name)
            .into()
    }
}

impl From<SettingsExpander> for Element {
    fn from(expander: SettingsExpander) -> Self {
        expander.into_element()
    }
}

fn render_card(card: SettingsCard, surface: CardSurface) -> Element {
    let SettingsCard {
        header,
        description,
        header_icon,
        content,
        content_alignment,
        is_click_enabled,
        on_click,
        action_icon,
        is_action_icon_visible,
    } = card;

    let has_header = !header.is_empty() || description.is_some() || header_icon.is_some();
    let header_icon = header_icon_element(header_icon);
    let description = description
        .map(|description| {
            text_block(description)
                .font_size(12.0)
                .foreground(tokens::SecondaryText)
                .wrap()
                .max_lines(3)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .into()
        })
        .unwrap_or(Element::Empty);
    let details = vstack((
        text_block(header.clone())
            .font_size(14.0)
            .semibold()
            .max_lines(1)
            .text_trimming(TextTrimming::CharacterEllipsis),
        description,
    ))
    .spacing(4.0)
    .vertical_alignment(VerticalAlignment::Center);

    let content = content.unwrap_or_default();
    let action_icon = if is_click_enabled && is_action_icon_visible {
        Some(action_icon.unwrap_or_else(default_action_icon))
    } else {
        None
    };
    let layout = match content_alignment {
        SettingsCardContentAlignment::Right => {
            right_aligned_layout(header_icon, details, content, action_icon, has_header)
        }
        SettingsCardContentAlignment::Left => left_aligned_layout(content, action_icon),
        SettingsCardContentAlignment::Vertical => {
            vertical_layout(header_icon, details, content, action_icon, has_header)
        }
    };

    let (padding, min_height, min_width, background, border_brush, border_thickness, radius) =
        match surface {
            CardSurface::Default => (
                Thickness::uniform(16.0),
                68.0,
                148.0,
                Some(tokens::CardBackground),
                Some(tokens::CardStroke),
                Some(Thickness::uniform(1.0)),
                8.0,
            ),
            CardSurface::ExpanderHeader => (
                Thickness {
                    left: 16.0,
                    top: 16.0,
                    right: 4.0,
                    bottom: 16.0,
                },
                68.0,
                0.0,
                None,
                None,
                None,
                0.0,
            ),
            CardSurface::ExpanderItem => (
                Thickness {
                    left: 58.0,
                    top: 8.0,
                    right: if is_click_enabled { 16.0 } else { 44.0 },
                    bottom: 8.0,
                },
                52.0,
                0.0,
                Some(tokens::CardBackground),
                Some(tokens::CardStroke),
                Some(Thickness {
                    left: 0.0,
                    top: 1.0,
                    right: 0.0,
                    bottom: 0.0,
                }),
                0.0,
            ),
        };

    let mut result = border(layout)
        .min_width(min_width)
        .min_height(min_height)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .padding(padding)
        .corner_radius(radius)
        .automation_name(if header.is_empty() {
            "设置项"
        } else {
            &header
        });

    if let Some(background) = background {
        result = result.background(background);
    }
    if let Some(border_brush) = border_brush {
        result = result.border_brush(border_brush);
    }
    if let Some(border_thickness) = border_thickness {
        result = result.border_thickness(border_thickness);
    }
    if is_click_enabled {
        if let Some(on_click) = on_click {
            result = result.on_tapped(on_click);
        }
    }

    result.into()
}

fn header_icon_element(icon: Option<Element>) -> Border {
    match icon {
        Some(icon) => border(icon)
            .width(20.0)
            .height(20.0)
            .margin(Thickness {
                left: 2.0,
                top: 0.0,
                right: 20.0,
                bottom: 0.0,
            })
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(0),
        None => border(text_block("")).width(0.0).height(1.0).grid_column(0),
    }
}

fn default_action_icon() -> Element {
    text_block("\u{E76C}")
        .font_family("Segoe Fluent Icons")
        .font_size(16.0)
        .foreground(tokens::SecondaryText)
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}

fn action_icon_element(icon: Element) -> Border {
    border(icon)
        .width(24.0)
        .height(24.0)
        .margin(Thickness {
            left: 14.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        })
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
}

fn right_aligned_layout(
    icon: Border,
    details: StackPanel,
    content: Element,
    action_icon: Option<Element>,
    has_header: bool,
) -> Element {
    let has_content = !matches!(content, Element::Empty);
    let mut children = Vec::new();
    let mut columns = Vec::new();

    if has_header {
        children.push(icon.into());
        children.push(details.grid_column(1).into());
        columns.extend([GridLength::Auto, GridLength::STAR]);
    }

    if has_content {
        let column = columns.len();
        let content = grid((border(content)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(1),))
        .columns([GridLength::STAR, GridLength::Auto])
        .min_width(if has_header { 120.0 } else { 0.0 })
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Center);
        children.push(content.grid_column(column as i32).into());
        columns.push(if has_header {
            GridLength::Auto
        } else {
            GridLength::STAR
        });
    }

    if let Some(action_icon) = action_icon {
        let column = columns.len();
        children.push(
            action_icon_element(action_icon)
                .grid_column(column as i32)
                .into(),
        );
        columns.push(GridLength::Auto);
    }

    if children.is_empty() {
        return Element::Empty;
    }

    grid(children)
        .columns(columns)
        .column_spacing(0.0)
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}

fn left_aligned_layout(content: Element, action_icon: Option<Element>) -> Element {
    let has_content = !matches!(content, Element::Empty);
    match action_icon {
        None if has_content => border(content)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
        None => Element::Empty,
        Some(action_icon) => {
            let mut children = Vec::new();
            let mut columns = Vec::new();

            if has_content {
                children.push(
                    border(content)
                        .horizontal_alignment(HorizontalAlignment::Left)
                        .vertical_alignment(VerticalAlignment::Center)
                        .grid_column(0)
                        .into(),
                );
                columns.push(GridLength::STAR);
            }

            children.push(
                action_icon_element(action_icon)
                    .grid_column(columns.len() as i32)
                    .into(),
            );
            columns.push(GridLength::Auto);

            grid(children)
                .columns(columns)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .vertical_alignment(VerticalAlignment::Center)
                .into()
        }
    }
}

fn vertical_layout(
    icon: Border,
    details: StackPanel,
    content: Element,
    action_icon: Option<Element>,
    has_header: bool,
) -> Element {
    let has_content = !matches!(content, Element::Empty);
    let has_header_row = has_header || action_icon.is_some();

    if !has_header_row {
        return if has_content {
            border(content)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .into()
        } else {
            Element::Empty
        };
    }

    let mut header_children = Vec::new();
    let mut header_columns = Vec::new();
    if has_header {
        header_children.push(icon.into());
        header_children.push(details.grid_column(1).into());
        header_columns.extend([GridLength::Auto, GridLength::STAR]);
    }
    if let Some(action_icon) = action_icon {
        header_children.push(
            action_icon_element(action_icon)
                .grid_column(header_columns.len() as i32)
                .into(),
        );
        header_columns.push(GridLength::Auto);
    }

    let header = grid(header_children)
        .columns(header_columns)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Center);
    if has_content {
        vstack((
            header,
            border(content).horizontal_alignment(HorizontalAlignment::Stretch),
        ))
        .spacing(8.0)
        .into()
    } else {
        header.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_alignment_defaults_to_right() {
        assert_eq!(
            SettingsCard::new("Network").content_alignment,
            SettingsCardContentAlignment::Right
        );
    }

    #[test]
    fn empty_descriptions_are_not_stored() {
        assert_eq!(SettingsCard::new("Network").description, None);
        assert_eq!(
            SettingsCard::new("Network").description("").description,
            None
        );
    }

    #[test]
    fn click_callbacks_enable_the_action_icon_by_default() {
        let card = SettingsCard::new("Open").on_click(|| {});

        assert!(card.is_click_enabled);
        assert!(card.is_action_icon_visible);
        assert!(card.action_icon.is_none());
    }

    #[test]
    fn action_icons_can_be_hidden_for_clickable_cards() {
        let card = SettingsCard::new("Open")
            .on_click(|| {})
            .is_action_icon_visible(false);

        assert!(!card.is_action_icon_visible);
    }
}
