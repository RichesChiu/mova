use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct AuthRateLimitSettings {
    pub max_failures: u32,
    pub window: Duration,
    pub lockout: Duration,
    pub max_keys: usize,
}

impl Default for AuthRateLimitSettings {
    fn default() -> Self {
        Self {
            max_failures: 5,
            window: Duration::from_secs(5 * 60),
            lockout: Duration::from_secs(15 * 60),
            max_keys: 4_096,
        }
    }
}

#[derive(Clone)]
pub struct AuthRateLimiter {
    settings: AuthRateLimitSettings,
    entries: Arc<Mutex<HashMap<String, AuthRateLimitEntry>>>,
}

impl Default for AuthRateLimiter {
    fn default() -> Self {
        Self::new(AuthRateLimitSettings::default())
    }
}

#[derive(Debug, Clone, Copy)]
struct AuthRateLimitEntry {
    window_started_at: Instant,
    failed_attempts: u32,
    blocked_until: Option<Instant>,
    last_seen_at: Instant,
}

impl AuthRateLimiter {
    pub fn new(settings: AuthRateLimitSettings) -> Self {
        Self {
            settings,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn check(&self, key: &str) -> Result<(), u64> {
        self.check_at(key, Instant::now())
    }

    pub fn record_failure(&self, key: &str) -> Option<u64> {
        self.record_failure_at(key, Instant::now())
    }

    pub fn record_success(&self, key: &str) {
        let mut entries = self
            .entries
            .lock()
            .expect("auth rate limiter lock poisoned");
        entries.remove(key);
    }

    fn check_at(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut entries = self
            .entries
            .lock()
            .expect("auth rate limiter lock poisoned");
        let Some(entry) = entries.get_mut(key) else {
            return Ok(());
        };
        entry.last_seen_at = now;

        if let Some(blocked_until) = entry.blocked_until {
            if blocked_until > now {
                return Err(remaining_seconds(now, blocked_until));
            }
            *entry = AuthRateLimitEntry::new(now);
            return Ok(());
        }

        if now.duration_since(entry.window_started_at) >= self.settings.window {
            *entry = AuthRateLimitEntry::new(now);
        }

        Ok(())
    }

    fn record_failure_at(&self, key: &str, now: Instant) -> Option<u64> {
        let mut entries = self
            .entries
            .lock()
            .expect("auth rate limiter lock poisoned");
        self.make_room(&mut entries, now, key);
        let entry = entries
            .entry(key.to_string())
            .or_insert_with(|| AuthRateLimitEntry::new(now));
        entry.last_seen_at = now;

        if let Some(blocked_until) = entry.blocked_until {
            if blocked_until > now {
                return Some(remaining_seconds(now, blocked_until));
            }
            *entry = AuthRateLimitEntry::new(now);
        } else if now.duration_since(entry.window_started_at) >= self.settings.window {
            *entry = AuthRateLimitEntry::new(now);
        }

        entry.failed_attempts = entry.failed_attempts.saturating_add(1);
        if entry.failed_attempts < self.settings.max_failures {
            return None;
        }

        let blocked_until = now + self.settings.lockout;
        entry.blocked_until = Some(blocked_until);
        Some(remaining_seconds(now, blocked_until))
    }

    fn make_room(
        &self,
        entries: &mut HashMap<String, AuthRateLimitEntry>,
        now: Instant,
        incoming_key: &str,
    ) {
        if entries.contains_key(incoming_key) || entries.len() < self.settings.max_keys {
            return;
        }

        let retention = self.settings.window.max(self.settings.lockout) * 2;
        entries.retain(|_, entry| {
            entry
                .blocked_until
                .is_some_and(|blocked_until| blocked_until > now)
                || now.duration_since(entry.last_seen_at) < retention
        });

        if entries.len() < self.settings.max_keys {
            return;
        }

        if let Some(oldest_key) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_seen_at)
            .map(|(key, _)| key.clone())
        {
            entries.remove(&oldest_key);
        }
    }
}

impl AuthRateLimitEntry {
    fn new(now: Instant) -> Self {
        Self {
            window_started_at: now,
            failed_attempts: 0,
            blocked_until: None,
            last_seen_at: now,
        }
    }
}

fn remaining_seconds(now: Instant, deadline: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(now);
    remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() > 0))
}

#[cfg(test)]
mod tests {
    use super::{AuthRateLimitSettings, AuthRateLimiter};
    use std::time::{Duration, Instant};

    fn test_limiter() -> AuthRateLimiter {
        AuthRateLimiter::new(AuthRateLimitSettings {
            max_failures: 3,
            window: Duration::from_secs(60),
            lockout: Duration::from_secs(120),
            max_keys: 2,
        })
    }

    #[test]
    fn blocks_at_the_configured_failure_threshold() {
        let limiter = test_limiter();
        let now = Instant::now();

        assert_eq!(limiter.record_failure_at("account:viewer", now), None);
        assert_eq!(limiter.record_failure_at("account:viewer", now), None);
        assert_eq!(limiter.record_failure_at("account:viewer", now), Some(120));
        assert_eq!(limiter.check_at("account:viewer", now), Err(120));
    }

    #[test]
    fn successful_authentication_clears_failures() {
        let limiter = test_limiter();
        let now = Instant::now();
        limiter.record_failure_at("account:viewer", now);
        limiter.record_failure_at("account:viewer", now);

        limiter.record_success("account:viewer");

        assert_eq!(limiter.check_at("account:viewer", now), Ok(()));
        assert_eq!(limiter.record_failure_at("account:viewer", now), None);
    }

    #[test]
    fn expired_lockout_starts_a_fresh_window() {
        let limiter = test_limiter();
        let now = Instant::now();
        limiter.record_failure_at("account:viewer", now);
        limiter.record_failure_at("account:viewer", now);
        limiter.record_failure_at("account:viewer", now);

        assert_eq!(
            limiter.check_at("account:viewer", now + Duration::from_secs(121)),
            Ok(())
        );
        assert_eq!(
            limiter.record_failure_at("account:viewer", now + Duration::from_secs(121)),
            None
        );
    }

    #[test]
    fn tracked_keys_remain_bounded() {
        let limiter = test_limiter();
        let now = Instant::now();
        limiter.record_failure_at("account:first", now);
        limiter.record_failure_at("account:second", now + Duration::from_secs(1));
        limiter.record_failure_at("account:third", now + Duration::from_secs(2));

        let entries = limiter
            .entries
            .lock()
            .expect("auth rate limiter lock poisoned");
        assert_eq!(entries.len(), 2);
        assert!(!entries.contains_key("account:first"));
    }
}
