use windows_reactor::*;

#[derive(Default)]
pub struct SettingsExpander {
    header: String,
    description: Option<String>,
    header_icon: Option<View>,
    header_content: Option<View>,
    content: Option<View>,
    items_header: Option<View>,
    items_footer: Option<View>,
    items: Vec<View>,
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

    pub fn description(mut self, value: impl Into<String>) -> Self {
        self.description = Some(value.into());
        self
    }

    pub fn header_icon(mut self, value: impl Into<View>) -> Self {
        self.header_icon = Some(value.into());
        self
    }

    /// Sets the control shown on the right side of the header.
    pub fn header_content(mut self, value: impl Into<View>) -> Self {
        self.header_content = Some(value.into());
        self
    }

    /// Sets a custom view shown when the expander is open.
    ///
    /// When omitted, [`Self::items`] are rendered in the content area.
    pub fn content(mut self, value: impl Into<View>) -> Self {
        self.content = Some(value.into());
        self
    }

    /// Sets content rendered above the expander items.
    pub fn items_header(mut self, value: impl Into<View>) -> Self {
        self.items_header = Some(value.into());
        self
    }

    /// Sets content rendered below the expander items.
    pub fn items_footer(mut self, value: impl Into<View>) -> Self {
        self.items_footer = Some(value.into());
        self
    }

    /// Sets the views rendered in the expander content area.
    pub fn items(mut self, values: impl IntoIterator<Item = View>) -> Self {
        self.items = values.into_iter().collect();
        self
    }

    pub fn is_expanded(mut self, value: bool) -> Self {
        self.is_expanded = value;
        self
    }

    pub fn on_expanding(mut self, callback: impl IntoPayloadCallback<bool>) -> Self {
        self.on_expanding = Some(callback.into_payload_callback());
        self
    }
}

impl From<SettingsExpander> for View {
    fn from(expander: SettingsExpander) -> Self {
        render_expander(expander)
    }
}

fn render_expander(expander: SettingsExpander) -> View {
    let SettingsExpander {
        header,
        description,
        header_icon,
        header_content,
        content,
        items_header,
        items_footer,
        items,
        is_expanded,
        on_expanding: on_is_expanded_changed,
    } = expander;

    // Keep the first grid column available for expanders without an icon.
    let mut icon = Border::new().width(0.0).height(1.0);
    if let Some(icon_view) = header_icon {
        icon = icon
            .width(20.0)
            .height(20.0)
            .margin(Thickness::new(2.0, 0.0, 20.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .content(icon_view);
    }
    let icon: View = icon.into();

    let mut detail_children: Vec<View> = vec![
        TextBlock::new()
            .text(header)
            .font_size(14.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .into(),
    ];
    if let Some(value) = description {
        detail_children.push(
            TextBlock::new()
                .text(value)
                .font_size(12.0)
                .foreground(Color::rgb(120, 120, 120))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        );
    }

    // Add the optional description only when it exists.
    let details: View = StackPanel::new()
        .spacing(4.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children(detail_children)
        .into();

    // Keep the trailing grid column, but omit its content relation when unused.
    let mut trailing = Border::new()
        .horizontal_alignment(HorizontalAlignment::Right)
        .vertical_alignment(VerticalAlignment::Center);
    if let Some(content) = header_content {
        trailing = trailing.content(content);
    }
    let trailing: View = trailing.into();

    let header = Border::new()
        .min_height(68.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .padding(Thickness::new(16.0, 16.0, 4.0, 16.0))
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    Border::new().grid_column(0).content(icon),
                    Border::new().grid_column(1).content(details),
                    Border::new().grid_column(2).content(trailing),
                )),
        );

    let items: View = StackPanel::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .keyed_children(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| KeyedView::new(index, item)),
        )
        .into();

    let content = content.unwrap_or(items);
    let mut content_children = Vec::with_capacity(3);
    if let Some(items_header) = items_header {
        content_children.push(KeyedView::new(
            "header",
            Border::new()
                .grid_row(0)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .content(items_header),
        ));
    }
    content_children.push(KeyedView::new(
        "items",
        Border::new()
            .grid_row(1)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .content(content),
    ));
    if let Some(items_footer) = items_footer {
        content_children.push(KeyedView::new(
            "footer",
            Border::new()
                .grid_row(2)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .content(items_footer),
        ));
    }
    let content: View = Grid::new()
        .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Top)
        .keyed_children(content_children)
        .into();

    // WinUI's stock Expander template resolves ExpanderContentPadding as a
    // StaticResource, so a local resource override cannot replace its 16px
    // value. Pull the content back over that template padding instead; the
    // native content border (one pixel) remains visible around the rows.
    let content: View = Border::new()
        .margin(Thickness::uniform(-16.0))
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .content(content)
        .into();

    let mut expander = Expander::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .horizontal_content_alignment(HorizontalAlignment::Stretch)
        .is_expanded(is_expanded);
    if let Some(callback) = on_is_expanded_changed {
        expander = expander.on_is_expanded_changed(callback);
    }

    Border::new()
        .min_width(148.0)
        .min_height(68.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Top)
        // Expander supplies its own background, border, and corner radius.
        .content(expander.header(header).content(content))
        .into()
}
