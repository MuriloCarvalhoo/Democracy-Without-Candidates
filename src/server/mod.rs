//! HTTP composition: shared state, request context helpers and middleware.

pub mod context;
pub mod http;
pub mod startup;

use std::sync::Arc;

use crate::{
    application::session::SessionPolicy,
    infrastructure::{clock::Clock, config::Config, db::Db, face::FaceVerifier, flags::Flags},
};

/// Process-wide state provided to every SSR render and server function.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub flags: Arc<Flags>,
    pub clock: Arc<dyn Clock>,
    pub face: FaceVerifier,
}

impl AppState {
    pub fn session_policy(&self) -> SessionPolicy {
        SessionPolicy::from_secs(
            self.config.session_idle_secs,
            self.config.session_absolute_secs,
        )
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    /// Instance timezone from the country configuration (civil year, display).
    pub fn tz(&self) -> chrono_tz::Tz {
        self.config
            .country
            .timezone
            .parse()
            .unwrap_or(chrono_tz::UTC)
    }

    pub fn proposal_ctx(&self) -> crate::application::proposals::Ctx<'_> {
        crate::application::proposals::Ctx {
            db: &self.db,
            flags: &self.flags,
            now: self.now(),
            tz: self.tz(),
        }
    }
}
