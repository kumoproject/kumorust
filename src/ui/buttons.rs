use windows_reactor::*;

/// Icon + label content for buttons and cards.
pub fn icon_content(symbol: Symbol, label: impl Into<String>) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(6.0)
        .children((SymbolIcon::new().symbol(symbol), label.into()))
}
