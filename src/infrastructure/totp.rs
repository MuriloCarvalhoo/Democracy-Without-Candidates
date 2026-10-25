//! TOTP (RFC 6238): 6 digits, 30 s, one step of skew each side. Replay of an
//! accepted step is blocked by the caller through `last_step`.

use rand::RngCore;
use totp_rs::{Builder, Secret, Totp};

use crate::error::AppError;

pub const SECRET_LEN: usize = 20;
pub const ISSUER: &str = "Democracia Sem Candidatos";

pub fn new_secret() -> Vec<u8> {
    let mut secret = vec![0u8; SECRET_LEN];
    rand::rng().fill_bytes(&mut secret);
    secret
}

fn totp(secret: &[u8], account: &str) -> Result<Totp, AppError> {
    Builder::new()
        .with_secret(secret.to_vec())
        .with_account_name(account.replace(':', ""))
        .with_issuer(Some(ISSUER))
        .build()
        .map_err(AppError::internal)
}

/// Accepted step for `code` at `now_unix`, if it is newer than `last_step`.
pub fn verify(secret: &[u8], code: &str, now_unix: i64, last_step: Option<i64>) -> Option<i64> {
    let code = code.trim();
    if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let step = totp(secret, "x")
        .ok()?
        .check(code, u64::try_from(now_unix).ok()?)?;
    let step = i64::try_from(step).ok()?;
    (step > last_step.unwrap_or(-1)).then_some(step)
}

/// Code for `at_unix` (fixtures and tests).
pub fn code_at(secret: &[u8], at_unix: i64) -> String {
    totp(secret, "x")
        .map(|t| t.generate(at_unix.max(0) as u64).to_string())
        .unwrap_or_default()
}

pub struct Provisioning {
    pub secret_base32: String,
    pub otpauth_uri: String,
    pub qr_svg: String,
}

pub fn provisioning(secret: &[u8], account: &str) -> Result<Provisioning, AppError> {
    let t = totp(secret, account)?;
    let uri = t.to_url().map_err(AppError::internal)?;
    let qr_svg = qrcode::QrCode::new(uri.as_bytes())
        .map(|code| {
            code.render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .build()
        })
        .map_err(AppError::internal)?;
    Ok(Provisioning {
        secret_base32: Secret::from(secret.to_vec()).to_base32(),
        otpauth_uri: uri,
        qr_svg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_and_replay() {
        let s = new_secret();
        let now = 1_800_000_000;
        let step = now / 30;
        assert_eq!(verify(&s, &code_at(&s, now), now, None), Some(step));
        assert_eq!(
            verify(&s, &code_at(&s, now - 30), now, None),
            Some(step - 1)
        );
        assert_eq!(
            verify(&s, &code_at(&s, now + 30), now, None),
            Some(step + 1)
        );
        assert_eq!(verify(&s, &code_at(&s, now - 60), now, None), None);
        assert_eq!(
            verify(&s, &code_at(&s, now), now, Some(step)),
            None,
            "same step replay"
        );
        assert_eq!(verify(&s, "12345", now, None), None);
        assert_eq!(verify(&s, "abcdef", now, None), None);
    }

    #[test]
    fn provisioning_uri_and_qr() {
        let s = new_secret();
        let p = provisioning(&s, "ana+x@example.org").unwrap();
        assert!(p.otpauth_uri.starts_with("otpauth://totp/"));
        assert!(
            p.otpauth_uri
                .contains(&format!("secret={}", p.secret_base32))
        );
        assert!(p.qr_svg.contains("<svg"));
    }
}
