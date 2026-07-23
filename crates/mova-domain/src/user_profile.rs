use crate::User;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryVisibility<'a> {
    All,
    Restricted(&'a [i64]),
}

impl<'a> LibraryVisibility<'a> {
    pub fn restricted_library_ids(self) -> Option<&'a [i64]> {
        match self {
            Self::All => None,
            Self::Restricted(library_ids) => Some(library_ids),
        }
    }

    pub fn allows_library(self, library_id: i64) -> bool {
        match self {
            Self::All => true,
            Self::Restricted(library_ids) => library_ids.contains(&library_id),
        }
    }
}

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

    pub fn library_visibility(&self) -> LibraryVisibility<'_> {
        if self.is_admin() {
            LibraryVisibility::All
        } else {
            LibraryVisibility::Restricted(&self.library_ids)
        }
    }

    pub fn can_access_library(&self, library_id: i64) -> bool {
        self.library_visibility().allows_library(library_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{LibraryVisibility, UserProfile};
    use crate::{User, UserRole};
    use time::OffsetDateTime;

    fn test_user(role: UserRole, library_ids: Vec<i64>) -> UserProfile {
        let now = OffsetDateTime::UNIX_EPOCH;

        UserProfile {
            user: User {
                id: 7,
                username: "viewer".to_string(),
                nickname: "Viewer".to_string(),
                role,
                is_enabled: true,
                created_at: now,
                updated_at: now,
            },
            is_primary_admin: false,
            library_ids,
        }
    }

    #[test]
    fn administrators_receive_all_libraries_even_without_explicit_grants() {
        let user = test_user(UserRole::Admin, Vec::new());

        assert_eq!(user.library_visibility(), LibraryVisibility::All);
        assert!(user.can_access_library(99));
    }

    #[test]
    fn viewers_receive_only_explicitly_granted_libraries() {
        let user = test_user(UserRole::Viewer, vec![2, 5]);

        assert_eq!(
            user.library_visibility(),
            LibraryVisibility::Restricted(&[2, 5])
        );
        assert!(user.can_access_library(5));
        assert!(!user.can_access_library(7));
    }

    #[test]
    fn viewers_without_grants_receive_no_libraries() {
        let user = test_user(UserRole::Viewer, Vec::new());

        assert_eq!(
            user.library_visibility(),
            LibraryVisibility::Restricted(&[])
        );
        assert!(!user.can_access_library(1));
    }
}
