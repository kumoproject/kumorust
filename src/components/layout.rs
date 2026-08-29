use windows_reactor::*;

/// Vertical stack with the app-wide default spacing.
pub fn vstack(children: impl IntoViews) -> View {
    StackPanel::new().spacing(14.0).children(children)
}

/// Horizontal stack with the app-wide default spacing.
pub fn hstack(children: impl IntoViews) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children(children)
}

/// Icon + label content for buttons and cards.
pub fn icon_content(symbol: Symbol, label: impl Into<String>) -> View {
    hstack((SymbolIcon::new().symbol(symbol), label.into()))
}
