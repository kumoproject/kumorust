use windows_reactor::*;

use crate::core::i18n::tr;

/// Transient notice bar, or nothing when the notice is empty.
pub fn info_bar(notice: &str) -> View {
    if notice.is_empty() {
        View::empty()
    } else {
        InfoBar::new()
            .title(tr("common.notice"))
            .message(notice)
            .severity(InfoBarSeverity::Informational)
            .is_open(true)
            .is_closable(false)
            .into()
    }
}
