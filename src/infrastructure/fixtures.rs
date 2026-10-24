//! Synthetic development/test fixtures. Refused in production. Never creates fixed
//! production credentials; feature modules add their own synthetic data here.

use super::{config::Config, db::Db, flags};
use crate::error::AppError;

/// Flags enabled for local demonstration with synthetic data.
pub const DEV_FLAGS: &[&str] = &[
    flags::ADDRESS_BRASILAPI,
    flags::PROPOSALS,
    flags::SIGNATURES,
    flags::DEBATE,
    flags::VOTING,
];

pub async fn seed_dev(db: &Db, config: &Config) -> Result<Vec<String>, AppError> {
    if config.is_production() {
        return Err(AppError::Forbidden);
    }
    let mut done = Vec::new();
    for key in DEV_FLAGS {
        flags::store(db, key, true, Some("seed-dev")).await?;
        done.push(format!("flag {key}=on"));
    }
    Ok(done)
}
