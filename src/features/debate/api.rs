//! Debate server functions: public network endpoints. They resolve the caller on
//! the server, bound identifiers and delegate to `application::debate`.

use leptos::prelude::*;

use crate::{
    domain::debate::{
        AmendmentChange, ModerationDecision, ReportReason,
        view::{AmendmentsView, DebateView, ReportedComment, Revision},
    },
    error::AppError,
};

#[cfg(feature = "ssr")]
mod srv {
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

    /// On-read deadline check so phase rules use the current status.
    pub async fn advance(state: &AppState) -> Result<(), AppError> {
        phases::advance_due(&state.db, state.now())
            .await
            .map(|_| ())
    }
}

#[server(endpoint = "debate/thread")]
pub async fn get_debate(id: String, page: Option<u32>) -> Result<DebateView, AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    app::thread(
        &state.db.read().await?,
        &state.flags,
        state.now(),
        state.tz(),
        viewer.as_ref(),
        &id,
        page.unwrap_or(1).clamp(1, 10_000),
    )
    .await
}

#[server(endpoint = "debate/comment")]
pub async fn post_comment(
    id: String,
    parent_id: Option<String>,
    body: String,
) -> Result<String, AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&id)?;
    if let Some(parent) = &parent_id {
        srv::check_id(parent)?;
    }
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    app::post_comment(
        &state.proposal_ctx(),
        &actor,
        &id,
        parent_id.as_deref(),
        &body,
    )
    .await
}

#[server(endpoint = "debate/edit")]
pub async fn edit_comment(comment_id: String, body: String) -> Result<(), AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&comment_id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    app::edit_comment(&state.proposal_ctx(), &actor, &comment_id, &body).await
}

#[server(endpoint = "debate/history")]
pub async fn comment_history(comment_id: String) -> Result<Vec<Revision>, AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&comment_id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::history(
        &state.db.read().await?,
        &state.flags,
        state.tz(),
        &actor,
        &comment_id,
    )
    .await
}

#[server(endpoint = "debate/report")]
pub async fn report_comment(
    comment_id: String,
    reason: ReportReason,
    details: String,
) -> Result<(), AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&comment_id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::report_comment(&state.proposal_ctx(), &actor, &comment_id, reason, &details).await
}

#[server(endpoint = "debate/moderate")]
pub async fn moderate_comment(
    comment_id: String,
    decision: ModerationDecision,
    reason: String,
) -> Result<(), AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&comment_id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::moderate_comment(
        &state.proposal_ctx(),
        &actor,
        &comment_id,
        decision,
        &reason,
    )
    .await
}

#[server(endpoint = "debate/lock")]
pub async fn set_debate_lock(id: String, locked: bool, reason: String) -> Result<(), AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::set_lock(&state.proposal_ctx(), &actor, &id, locked, &reason).await
}

#[server(endpoint = "debate/amendments")]
pub async fn get_amendments(id: String) -> Result<AmendmentsView, AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    app::amendments(
        &state.db.read().await?,
        &state.flags,
        state.now(),
        state.tz(),
        viewer.as_ref(),
        &id,
    )
    .await
}

#[server(endpoint = "debate/amend")]
pub async fn propose_amendment(
    id: String,
    title: String,
    body: String,
) -> Result<String, AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    app::propose_amendment(&state.proposal_ctx(), &actor, &id, &title, &body).await
}

#[server(endpoint = "debate/amendment_change")]
pub async fn change_amendment(
    amendment_id: String,
    change: AmendmentChange,
    note: String,
) -> Result<(), AppError> {
    use crate::{application::debate as app, server::context};
    srv::check_id(&amendment_id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::change_amendment(&state.proposal_ctx(), &actor, &amendment_id, change, &note).await
}

#[server(endpoint = "debate/moderation_queue")]
pub async fn moderation_queue() -> Result<Vec<ReportedComment>, AppError> {
    use crate::{application::debate as app, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    app::moderation_queue(&state.db.read().await?, &state.flags, state.tz(), &actor).await
}
