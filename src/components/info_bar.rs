use windows_reactor::*;

/// Transient notice bar, or nothing when the notice is empty.
pub fn info_bar(notice: &str) -> View {
    if notice.is_empty() {
        View::empty()
    } else {
        InfoBar::new()
            .title("提示")
            .message(notice)
            .severity(InfoBarSeverity::Informational)
            .is_open(true)
            .is_closable(false)
            .into()
    }
}
