use super::errors::{RemoteStreamError, RemoteStreamErrorKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub const DEFAULT_GLOBAL_STREAM_LIMIT: usize = 64;
pub const DEFAULT_PER_USER_STREAM_LIMIT: usize = 4;

#[derive(Clone)]
pub(crate) struct RemoteStreamLimits {
    global: Arc<Semaphore>,
    users: Arc<Mutex<HashMap<i64, Weak<Semaphore>>>>,
    per_user_limit: usize,
}

#[derive(Debug)]
pub(crate) struct RemoteStreamPermit {
    _user: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

impl RemoteStreamLimits {
    pub(crate) fn new(global_limit: usize, per_user_limit: usize) -> Self {
        assert!(
            global_limit > 0,
            "global STRM stream limit must be positive"
        );
        assert!(
            per_user_limit > 0,
            "per-user STRM stream limit must be positive"
        );
        Self {
            global: Arc::new(Semaphore::new(global_limit)),
            users: Arc::new(Mutex::new(HashMap::new())),
            per_user_limit,
        }
    }

    pub(crate) fn try_acquire(
        &self,
        user_id: i64,
    ) -> Result<RemoteStreamPermit, RemoteStreamError> {
        let user = {
            let mut users = self.users.lock().expect("STRM user limit lock poisoned");
            users.retain(|_, semaphore| semaphore.strong_count() > 0);
            users
                .entry(user_id)
                .or_default()
                .upgrade()
                .unwrap_or_else(|| {
                    let semaphore = Arc::new(Semaphore::new(self.per_user_limit));
                    users.insert(user_id, Arc::downgrade(&semaphore));
                    semaphore
                })
        };

        let user_permit = user.try_acquire_owned().map_err(|_| {
            RemoteStreamError::new(
                RemoteStreamErrorKind::UserLimitExceeded,
                "the user already has the maximum number of STRM streams",
            )
        })?;
        let global_permit = self.global.clone().try_acquire_owned().map_err(|_| {
            RemoteStreamError::new(
                RemoteStreamErrorKind::CapacityExhausted,
                "the server has no remaining STRM streaming capacity",
            )
        })?;

        Ok(RemoteStreamPermit {
            _user: user_permit,
            _global: global_permit,
        })
    }
}

impl Default for RemoteStreamLimits {
    fn default() -> Self {
        Self::new(DEFAULT_GLOBAL_STREAM_LIMIT, DEFAULT_PER_USER_STREAM_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_per_user_limit_and_releases_on_drop() {
        let limits = RemoteStreamLimits::new(8, 2);
        let first = limits.try_acquire(7).unwrap();
        let second = limits.try_acquire(7).unwrap();
        let error = limits.try_acquire(7).unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::UserLimitExceeded);

        drop(first);
        let replacement = limits.try_acquire(7).unwrap();
        drop((second, replacement));
    }

    #[test]
    fn enforces_global_limit_across_users_and_releases_on_drop() {
        let limits = RemoteStreamLimits::new(2, 2);
        let first = limits.try_acquire(1).unwrap();
        let second = limits.try_acquire(2).unwrap();
        let error = limits.try_acquire(3).unwrap_err();
        assert_eq!(error.kind(), RemoteStreamErrorKind::CapacityExhausted);

        drop(first);
        let replacement = limits.try_acquire(3).unwrap();
        drop((second, replacement));
    }
}
