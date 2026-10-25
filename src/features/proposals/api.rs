//! Proposal server functions: public network endpoints. They resolve the caller
//! on the server, bound input sizes and delegate to the application use cases.

use leptos::prelude::*;
use leptos::server_fn::codec::Json;

use crate::{
    domain::{
        proposals::view::{
            Dashboard, Draft, DraftInput, DraftSaved, ProposalDetail, ProposalPage, Published,
            SetupVersionView, SignatureState, Signers,
        },
        setup::SetupParams,
    },
    error::AppError,
};

#[cfg(feature = "ssr")]
pub(crate) mod srv {
    use crate::{
        application::{actor::Actor, phases},
        error::AppError,
        server::{AppState, context},
    };

    pub fn check_id(id: &str) -> Result<(), AppError> {
        if !id.is_empty()
            && id.len() <= 64
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }

    /// Optional caller for public reads; an expired session reads as a visitor.
    pub async fn viewer() -> Result<Option<Actor>, AppError> {
        match context::current_actor(true).await {
            Err(AppError::SessionExpired) => Ok(None),
            other => other,
        }
    }

    /// On-read deadline check (same idempotent use case as the ticker).
    pub async fn advance(state: &AppState) -> Result<(), AppError> {
        phases::advance_due(&state.db, state.now())
            .await
            .map(|_| ())
    }
}

#[server(endpoint = "proposals/list")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(kind = ?kind, status = ?status, level = ?level, page = ?page), err(level = "debug")))]
pub async fn list_proposals(
    kind: Option<String>,
    status: Option<String>,
    level: Option<String>,
    page: Option<u32>,
) -> Result<ProposalPage, AppError> {
    use crate::{application::proposals, server::context};
    let state = context::state()?;
    let filter =
        proposals::ListFilter::parse(kind.as_deref(), status.as_deref(), level.as_deref(), page)?;
    srv::advance(&state).await?;
    proposals::list(&state.db.read().await?, state.tz(), &filter).await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn get_proposal(id: String) -> Result<ProposalDetail, AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    let conn = state.db.read().await?;
    proposals::detail(
        &conn,
        &state.flags,
        state.now(),
        state.tz(),
        viewer.as_ref(),
        &id,
    )
    .await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn get_draft(id: String) -> Result<Draft, AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    proposals::get_draft(&state.db.read().await?, &actor, &id).await
}

/// Effective setup parameters for the caller's municipality, or `None` when no
/// setup was adopted (laws blocked). Prefills meta-law drafts.
#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn setup_defaults() -> Result<Option<SetupParams>, AppError> {
    use crate::{application::setup, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let conn = state.db.read().await?;
    Ok(setup::effective_setup(&conn, &actor.municipality_id)
        .await?
        .map(|s| s.params))
}

#[server(input = Json, endpoint = "proposals/save_draft")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = ?id), err(level = "debug")))]
pub async fn save_draft(id: Option<String>, input: DraftInput) -> Result<DraftSaved, AppError> {
    use crate::{application::proposals, server::context};
    if let Some(id) = &id {
        srv::check_id(id)?;
    }
    let state = context::state()?;
    let actor = context::require_actor().await?;
    proposals::save_draft(&state.proposal_ctx(), &actor, id.as_deref(), &input).await
}

#[server(endpoint = "proposals/publish")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn publish_proposal(
    id: String,
    idempotency_key: Option<String>,
) -> Result<Published, AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    proposals::publish(
        &state.proposal_ctx(),
        &actor,
        &id,
        idempotency_key.as_deref(),
    )
    .await
}

#[server(endpoint = "proposals/sign")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn sign_proposal(
    id: String,
    idempotency_key: Option<String>,
) -> Result<SignatureState, AppError> {
    use crate::{application::signatures, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    signatures::sign(
        &state.proposal_ctx(),
        &actor,
        &id,
        idempotency_key.as_deref(),
    )
    .await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn unsign_proposal(
    id: String,
    idempotency_key: Option<String>,
) -> Result<SignatureState, AppError> {
    use crate::{application::signatures, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    signatures::withdraw_signature(
        &state.proposal_ctx(),
        &actor,
        &id,
        idempotency_key.as_deref(),
    )
    .await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn withdraw_proposal(id: String, reason: String) -> Result<(), AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    proposals::withdraw(&state.proposal_ctx(), &actor, &id, &reason).await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn archive_proposal(id: String, reason: String) -> Result<(), AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    proposals::archive(&state.proposal_ctx(), &actor, &id, &reason).await
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn proposal_signers(id: String) -> Result<Signers, AppError> {
    use crate::{application::proposals, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    proposals::signers(&state.db.read().await?, &id).await
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn dashboard_data() -> Result<Dashboard, AppError> {
    use crate::{application::proposals, server::context};
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    proposals::dashboard(&state.db.read().await?, state.tz(), viewer.as_ref()).await
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn setup_history() -> Result<Vec<SetupVersionView>, AppError> {
    use crate::{application::setup, server::context};
    let state = context::state()?;
    setup::history(&state.db.read().await?).await
}

/// `GET /propostas/{id}/exportar`: Markdown download of the published text + hash.
#[cfg(feature = "ssr")]
pub async fn export_handler(
    axum::Extension(state): axum::Extension<crate::server::AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::{
        http::{StatusCode, header},
        response::IntoResponse,
    };
    let result = async {
        srv::check_id(&id)?;
        let conn = state.db.read().await?;
        crate::application::proposals::export_markdown(&conn, state.tz(), &id).await
    }
    .await;
    match result {
        Ok(markdown) => (
            [
                (
                    header::CONTENT_TYPE,
                    "text/markdown; charset=utf-8".to_owned(),
                ),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"proposta-{id}.md\""),
                ),
            ],
            markdown,
        )
            .into_response(),
        Err(AppError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
