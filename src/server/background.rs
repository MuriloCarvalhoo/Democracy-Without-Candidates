//! Background loops owned by the server: a panic is logged and the loop
//! restarts (a detached task would die silently), and shutdown stops them all.

use std::{future::Future, time::Duration};

use tokio::task::{JoinHandle, JoinSet};

/// Pause before restarting a loop that panicked (avoids a hot panic loop).
const RESTART_DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Background {
    tasks: JoinSet<()>,
}

/// Aborts the supervised task when the supervisor itself is aborted.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Background {
    /// Runs `start()`; when the future panics, logs it and starts a new one
    /// after [`RESTART_DELAY`]. A future that returns ends the task.
    pub fn supervise<F, Fut>(&mut self, name: &'static str, start: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.supervise_with_delay(name, RESTART_DELAY, start);
    }

    fn supervise_with_delay<F, Fut>(&mut self, name: &'static str, delay: Duration, start: F)
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(async move {
            loop {
                let mut run = AbortOnDrop(tokio::spawn(start()));
                match (&mut run.0).await {
                    Ok(()) => return,
                    Err(e) if e.is_panic() => {
                        tracing::error!(task = name, "background task panicked; restarting");
                    }
                    Err(_) => return,
                }
                tokio::time::sleep(delay).await;
            }
        });
    }

    /// Stops every loop (after the HTTP server has drained) and waits for them.
    pub async fn shutdown(mut self) {
        self.tasks.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    /// Regression: a panicking loop is restarted instead of disappearing.
    #[tokio::test]
    async fn panicking_task_is_restarted() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut bg = Background::default();
        let counter = runs.clone();
        bg.supervise_with_delay("test", Duration::from_millis(1), move || {
            let counter = counter.clone();
            async move {
                if counter.fetch_add(1, Ordering::SeqCst) < 2 {
                    panic!("boom");
                }
                std::future::pending::<()>().await;
            }
        });
        for _ in 0..200 {
            if runs.load(Ordering::SeqCst) >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(runs.load(Ordering::SeqCst), 3, "two panics, two restarts");
        bg.shutdown().await;
    }

    /// Shutdown stops the supervised loop itself, not only its supervisor.
    #[tokio::test]
    async fn shutdown_stops_running_loops() {
        let ticks = Arc::new(AtomicUsize::new(0));
        let mut bg = Background::default();
        let counter = ticks.clone();
        bg.supervise("test", move || {
            let counter = counter.clone();
            async move {
                loop {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        bg.shutdown().await;
        let after = ticks.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(after > 0);
        assert_eq!(ticks.load(Ordering::SeqCst), after, "loop no longer runs");
    }
}
