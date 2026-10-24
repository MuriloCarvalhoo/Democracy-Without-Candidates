//! Face verification contract. Rust owns accounts and templates; the engine only
//! extracts/compares embeddings. Implementations:
//! - `Synthetic`: deterministic fake for development/test (rejected in production).
//! - `Service`: private Python engine on loopback (see `face-service/`).

use crate::error::AppError;

/// Opaque face template produced at enrollment. Must be encrypted at rest by the
/// identity use case; never logged or sent to the browser.
#[derive(Clone, PartialEq)]
pub struct FaceTemplate(pub Vec<u8>);

impl std::fmt::Debug for FaceTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FaceTemplate(<redacted>)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceOutcome {
    Match,
    NoMatch,
    /// No usable face (none/multiple faces, poor quality).
    Unusable,
}

/// Image captured in the browser for one verification attempt (JPEG/PNG bytes),
/// bound to a single-use server challenge.
pub struct FaceSample<'a> {
    pub image: &'a [u8],
    pub challenge: &'a str,
}

#[derive(Clone)]
pub enum FaceVerifier {
    Synthetic,
    Service(super::face_service::FaceServiceClient),
}

impl FaceVerifier {
    pub fn is_synthetic(&self) -> bool {
        matches!(self, Self::Synthetic)
    }

    /// Whether a match also proves a live person. Currently always `false`: no
    /// engine validates liveness server-side, so callers must not treat a match
    /// as liveness evidence.
    pub fn liveness_validated(&self) -> bool {
        match self {
            Self::Synthetic => false,
            Self::Service(client) => client.liveness_validated(),
        }
    }

    pub async fn enroll(&self, sample: FaceSample<'_>) -> Result<FaceTemplate, AppError> {
        match self {
            Self::Synthetic => synthetic::enroll(sample),
            Self::Service(client) => client.enroll(sample).await,
        }
    }

    pub async fn verify(
        &self,
        template: &FaceTemplate,
        sample: FaceSample<'_>,
    ) -> Result<FaceOutcome, AppError> {
        match self {
            Self::Synthetic => synthetic::verify(template, sample),
            Self::Service(client) => client.verify(template, sample).await,
        }
    }
}

/// Synthetic engine: the "face" is any non-empty byte string; equality of its
/// SHA-256 decides the match. Proves nothing about real biometrics.
mod synthetic {
    use super::*;
    use crate::infrastructure::crypto::sha256_hex;

    pub fn enroll(sample: FaceSample<'_>) -> Result<FaceTemplate, AppError> {
        if sample.image.is_empty() {
            return Err(AppError::validation("face", "imagem vazia"));
        }
        Ok(FaceTemplate(sha256_hex(sample.image).into_bytes()))
    }

    pub fn verify(
        template: &FaceTemplate,
        sample: FaceSample<'_>,
    ) -> Result<FaceOutcome, AppError> {
        if sample.image.is_empty() {
            return Ok(FaceOutcome::Unusable);
        }
        Ok(if sha256_hex(sample.image).into_bytes() == template.0 {
            FaceOutcome::Match
        } else {
            FaceOutcome::NoMatch
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn synthetic_matches_same_bytes_only() {
        let v = FaceVerifier::Synthetic;
        let t = v
            .enroll(FaceSample {
                image: b"face-a",
                challenge: "c",
            })
            .await
            .unwrap();
        assert_eq!(
            v.verify(
                &t,
                FaceSample {
                    image: b"face-a",
                    challenge: "c"
                }
            )
            .await
            .unwrap(),
            FaceOutcome::Match
        );
        assert_eq!(
            v.verify(
                &t,
                FaceSample {
                    image: b"face-b",
                    challenge: "c"
                }
            )
            .await
            .unwrap(),
            FaceOutcome::NoMatch
        );
        assert_eq!(
            v.verify(
                &t,
                FaceSample {
                    image: b"",
                    challenge: "c"
                }
            )
            .await
            .unwrap(),
            FaceOutcome::Unusable
        );
        assert!(
            v.enroll(FaceSample {
                image: b"",
                challenge: "c"
            })
            .await
            .is_err()
        );
        assert_eq!(format!("{t:?}"), "FaceTemplate(<redacted>)");
    }
}
