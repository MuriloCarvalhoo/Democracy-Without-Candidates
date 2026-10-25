//! Argon2id password hashing (m = 19 MiB, t = 2, p = 1, random salt), run on the
//! blocking pool with at most one hash per CPU at a time (a login burst queues
//! instead of exhausting memory and the blocking pool). Passwords are never
//! logged or returned.

use std::sync::LazyLock;

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use tokio::sync::Semaphore;

use crate::error::AppError;

/// Concurrent Argon2 computations (each uses 19 MiB and one CPU for its duration).
static PERMITS: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(std::thread::available_parallelism().map_or(2, |n| n.get())));

/// Runs CPU-heavy work on the blocking pool once a permit is free.
async fn limited<T: Send + 'static>(
    permits: &'static Semaphore,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AppError> {
    let permit = permits.acquire().await.map_err(AppError::internal)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(AppError::internal)
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub async fn hash(password: &str) -> Result<String, AppError> {
    let password = password.to_owned();
    limited(&PERMITS, move || {
        argon2()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(AppError::internal)
    })
    .await?
}

/// Verifies against a stored PHC string. `None` runs a dummy verification so that
/// unknown accounts take comparable time.
pub async fn verify(stored: Option<String>, password: &str) -> bool {
    let password = password.to_owned();
    limited(&PERMITS, move || {
        let stored = stored.unwrap_or_else(|| DUMMY.to_owned());
        let ok = PasswordHash::new(&stored)
            .map(|h| argon2().verify_password(password.as_bytes(), &h).is_ok())
            .unwrap_or(false);
        ok && stored != DUMMY
    })
    .await
    .unwrap_or(false)
}

/// Valid Argon2id hash of a random value; used only to equalize timing.
const DUMMY: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Jr5HnYkD1Wm2pX8qsc6mMfV5vFFQjvzq3n1vP9TkC2E";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hashes_with_random_salt_and_verifies() {
        let a = hash("correct horse battery").await.unwrap();
        let b = hash("correct horse battery").await.unwrap();
        assert!(a.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert_ne!(a, b, "random salt");
        assert!(verify(Some(a.clone()), "correct horse battery").await);
        assert!(!verify(Some(a), "wrong password!!").await);
        assert!(!verify(None, "anything at all").await);
    }

    /// No more than the permits run at once, however many callers arrive.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrency_is_capped_by_permits() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static TWO: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
        static RUNNING: AtomicUsize = AtomicUsize::new(0);
        static PEAK: AtomicUsize = AtomicUsize::new(0);
        let tasks: Vec<_> = (0..12)
            .map(|_| {
                tokio::spawn(limited(&TWO, || {
                    let now = RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
                    PEAK.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    RUNNING.fetch_sub(1, Ordering::SeqCst);
                }))
            })
            .collect();
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(PEAK.load(Ordering::SeqCst), 2);
    }
}
