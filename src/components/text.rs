use windows_reactor::{Color, FontWeight, TextBlock};

/// Secondary text color (WinUI-style muted gray).
pub const TEXT_SECONDARY: Color = Color::rgb(120, 120, 120);
/// Tertiary text color for metadata lines.
pub const TEXT_TERTIARY: Color = Color::rgb(96, 96, 96);

/// Page heading (28px semibold).
pub fn title(text: impl Into<String>) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(28.0)
        .font_weight(FontWeight::SEMI_BOLD)
}

/// Section heading (20px semibold).
pub fn subtitle(text: impl Into<String>) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(20.0)
        .font_weight(FontWeight::SEMI_BOLD)
}

/// Regular body text (14px).
pub fn body(text: impl Into<String>) -> TextBlock {
    TextBlock::new().text(text).font_size(14.0)
}

/// Small caption text (12px).
pub fn caption(text: impl Into<String>) -> TextBlock {
    TextBlock::new().text(text).font_size(12.0)
}
