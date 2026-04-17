use crate::User;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub user: User,
    pub is_primary_admin: bool,
    pub library_ids: Vec<i64>,
}

impl UserProfile {
    pub fn is_admin(&self) -> bool {
        self.user.role.is_admin()
    }

    pub fn can_access_library(&self, library_id: i64) -> bool {
        self.is_admin() || self.library_ids.contains(&library_id)
    }
}
