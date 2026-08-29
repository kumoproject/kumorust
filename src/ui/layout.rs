use windows_reactor::*;

/// Vertical stack with the app-wide default spacing.
pub fn vstack(children: impl IntoViews) -> View {
    StackPanel::new().spacing(14.0).children(children)
}
