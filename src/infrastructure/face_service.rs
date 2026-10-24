//! HTTP client for the private Python face engine on loopback.
//! Placeholder until the face-service integration lands: always unavailable.

use super::face::{FaceOutcome, FaceSample, FaceTemplate};
use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct FaceServiceClient {
    pub base_url: String,
}

impl FaceServiceClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    pub async fn enroll(&self, _sample: FaceSample<'_>) -> Result<FaceTemplate, AppError> {
        Err(AppError::Unavailable("serviço facial não integrado".into()))
    }

    pub async fn verify(
        &self,
        _template: &FaceTemplate,
        _sample: FaceSample<'_>,
    ) -> Result<FaceOutcome, AppError> {
        Err(AppError::Unavailable("serviço facial não integrado".into()))
    }
}
