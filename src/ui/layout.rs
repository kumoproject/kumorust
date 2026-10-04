use windows_reactor::*;

/// Vertical stack with stable keys for conditionally rendered children.
pub fn keyed_vstack(children: impl IntoIterator<Item = KeyedView>) -> View {
    StackPanel::new()
        .spacing(14.0)
        .keyed_children(children)
        .into()
}
