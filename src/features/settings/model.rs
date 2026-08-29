use crate::domain::update::UpdateStatus;

/// The settings slice's model.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsModel {
    pub folders: Vec<String>,
    pub update_status: UpdateStatus,
    pub folders_expanded: bool,
}

impl SettingsModel {
    pub fn new(folders: Vec<String>) -> Self {
        Self {
            folders,
            update_status: UpdateStatus::Idle,
            folders_expanded: true,
        }
    }
}
