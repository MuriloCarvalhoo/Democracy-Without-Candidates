//! Landing page and the foundation probe: a synthetic counter persisted in TursoDB
//! and changed through a server function after hydration.

use leptos::prelude::*;

use crate::{error::AppError, features::ui::action_error, i18n::t};

// Stable endpoints: used by the load test harness (tools/load).
#[server(endpoint = "get_counter")]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn get_counter() -> Result<i64, AppError> {
    use crate::{infrastructure::db::query_opt, server::context};
    let state = context::state()?;
    if state.config.is_production() {
        return Err(AppError::NotFound);
    }
    let conn = state.db.read().await?;
    let row = query_opt(
        &conn,
        "SELECT value FROM synthetic_counter WHERE id = 1",
        (),
    )
    .await?;
    Ok(match row {
        Some(row) => row.get(0)?,
        None => 0,
    })
}

#[server(endpoint = "increment_counter")]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn increment_counter() -> Result<i64, AppError> {
    use crate::{
        infrastructure::{clock::ts, db::query_opt},
        server::context,
    };
    let state = context::state()?;
    if state.config.is_production() {
        return Err(AppError::NotFound);
    }
    let tx = state.db.write().await?;
    tx.execute(
        "INSERT INTO synthetic_counter (id, value, updated_at) VALUES (1, 1, ?1)
         ON CONFLICT(id) DO UPDATE SET value = value + 1, updated_at = excluded.updated_at",
        [ts(state.now())],
    )
    .await?;
    let value: i64 = query_opt(&tx, "SELECT value FROM synthetic_counter WHERE id = 1", ())
        .await?
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
        .get(0)?;
    tx.commit().await?;
    Ok(value)
}

/// Proposal lifecycle shown on the landing page: (phase class, title key, hint key).
const LIFECYCLE: [(&str, &str, &str); 5] = [
    (
        "step-propose",
        "home.step.propose",
        "home.step.propose_hint",
    ),
    (
        "step-collect",
        "home.step.collect",
        "home.step.collect_hint",
    ),
    ("step-debate", "home.step.debate", "home.step.debate_hint"),
    ("step-vote", "home.step.vote", "home.step.vote_hint"),
    ("step-result", "home.step.result", "home.step.result_hint"),
];

#[component]
pub fn HomePage() -> impl IntoView {
    let increment = ServerAction::<IncrementCounter>::new();
    let counter = Resource::new(move || increment.version().get(), |_| get_counter());
    let info = expect_context::<crate::features::shell::InstanceResource>();
    let synthetic = move || info.get().and_then(|r| r.ok()).is_some_and(|i| i.synthetic);

    view! {
        <section class="page hero" aria-labelledby="home-title">
            <div class="hero-text">
                <h1 id="home-title">{t("home.title")}</h1>
                <p class="lead">{t("home.lead")}</p>
                <p class="row">
                    <a class="button" href="/propostas">{t("home.cta.proposals")}</a>
                    <a class="button secondary" href="/dashboard">{t("home.cta.dashboard")}</a>
                </p>
            </div>
            <div class="lifecycle-wrap">
                <h2 class="lifecycle-title">{t("home.steps.title")}</h2>
                <ol class="lifecycle" data-testid="lifecycle">
                    {LIFECYCLE.iter().map(|&(phase, title, hint)| view! {
                        <li class=phase>
                            <strong>{t(title)}</strong>
                            <span>{t(hint)}</span>
                        </li>
                    }).collect_view()}
                </ol>
            </div>
        </section>
        // Foundation probe: only in development/test instances.
        <Transition fallback=|| ()>
        <Show when=synthetic>
        <section class="card" aria-labelledby="counter-title">
            <h2 id="counter-title">{t("foundation.counter")}</h2>
            <Transition fallback=|| view! { <p>{t("common.loading")}</p> }>
                <p>
                    {t("foundation.confirmed")}": "
                    <output data-testid="counter-value">
                        {move || counter.get().map(|r| r.map(|v| v.to_string()).unwrap_or_else(|e| e.user_message()))}
                    </output>
                </p>
            </Transition>
            <ActionForm action=increment>
                <button type="submit" data-testid="counter-increment" disabled=move || increment.pending().get()>
                    {t("foundation.increment")}
                </button>
            </ActionForm>
            {move || action_error(increment.value().get())}
        </section>
        </Show>
        </Transition>
    }
}
