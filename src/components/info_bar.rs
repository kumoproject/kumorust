use windows_reactor::*;

pub fn info_bar(notice: &str) -> Element {
    if notice.is_empty() {
        Element::Empty
    } else {
        InfoBar::new("提示")
            .message(notice)
            .informational()
            .is_closable(false)
            .into()
    }
}
