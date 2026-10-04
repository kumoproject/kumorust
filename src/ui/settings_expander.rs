use windows_reactor::*;

#[derive(Default)]
pub struct SettingsExpander {
    header: String,
    description: Option<String>,
    header_icon: Option<View>,
    content: Option<View>,
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
    pub fn content(mut self, value: impl Into<View>) -> Self {
        self.content = Some(value.into());
        self
    }

    /// Sets the views shown when the expander is open.
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
        content,
        items,
        is_expanded,
        on_expanding: on_is_expanded_changed,
    } = expander;

    let icon = match header_icon {
        Some(icon) => Border::new()
            .width(20.0)
            .height(20.0)
            .margin(Thickness::new(2.0, 0.0, 20.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .content(icon),
        None => Border::new().width(0.0).height(1.0).content(View::empty()),
    };

    let description: View = match description {
        Some(value) => TextBlock::new()
            .text(value)
            .font_size(12.0)
            .foreground(Color::rgb(120, 120, 120))
            .text_wrapping(TextWrapping::Wrap)
            .into(),
        None => View::empty(),
    };

    let details = StackPanel::new().spacing(4.0).children((
        TextBlock::new()
            .text(header)
            .font_size(14.0)
            .font_weight(FontWeight::SEMI_BOLD),
        description,
    ));

    let trailing = Border::new()
        .horizontal_alignment(HorizontalAlignment::Right)
        .vertical_alignment(VerticalAlignment::Center)
        .content(content.unwrap_or_else(View::empty));

    let header = Border::new()
        .min_height(68.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .padding(Thickness::xy(0.0, 16.0))
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    Border::new().grid_column(0).content(icon),
                    Border::new().grid_column(1).content(details),
                    Border::new().grid_column(2).content(trailing),
                )),
        );

    let items = StackPanel::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .keyed_children(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| KeyedView::new(index, item)),
        );

    let mut expander = Expander::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .is_expanded(is_expanded);
    if let Some(callback) = on_is_expanded_changed {
        expander = expander.on_is_expanded_changed(callback);
    }

    Border::new()
        .min_width(148.0)
        .min_height(68.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Top)
        // 因内部是 Expander 自带 background brush thickness corner_radius 等故无需重复设置
        .content(expander.slots([
            SlotView::new(ExpanderSlot::Header, header),
            SlotView::new(ExpanderSlot::Content, items),
        ]))
}
