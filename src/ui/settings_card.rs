use windows_reactor::*;

#[derive(Default)]
pub struct SettingsCard {
    header: String,
    header_icon: Option<View>,
    description: Option<String>,
    content: Option<View>,
}

impl SettingsCard {
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

    pub fn content(mut self, value: impl Into<View>) -> Self {
        self.content = Some(value.into());
        self
    }
}

impl From<SettingsCard> for View {
    fn from(card: SettingsCard) -> Self {
        render_card(card)
    }
}

fn render_card(card: SettingsCard) -> View {
    let SettingsCard {
        header,
        description,
        header_icon,
        content,
    } = card;

    // Keep the first grid column available for cards without an icon.
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
    if let Some(text) = description {
        detail_children.push(
            TextBlock::new()
                .text(text)
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
        .vertical_alignment(VerticalAlignment::Center)
        .grid_column(2);
    if let Some(content) = content {
        trailing = trailing.content(content);
    }
    let trailing: View = trailing.into();

    let row: View = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            Border::new().grid_column(0).content(icon),
            Border::new().grid_column(1).content(details),
            Border::new().grid_column(2).content(trailing),
        ))
        .into();

    Border::new()
        .min_width(148.0)
        .min_height(68.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .vertical_alignment(VerticalAlignment::Top)
        .padding(16.0)
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .corner_radius(4.0)
        .content(row)
        .into()
}
