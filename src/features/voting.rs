//! Demonstrative public vote, results, initial plebiscite and statistics screens.
//! Secret voting is unavailable (blocked until a validated protocol exists): the
//! page says so and requires explicit consent to a public vote.

use leptos::{prelude::*, task::spawn_local};
use leptos_router::{components::A, hooks::use_params_map};

use crate::{
    domain::{
        proposals::{Outcome, ProposalKind, RejectReason},
        voting::{
            Choice, format_bp, format_share,
            view::{
                CastConfirmation, ResultPage as ResultData, ResultState, ResultView, SetupDiff,
                SetupOverview, Statistics, VotePage as VoteData, VoteStatus, VoterState,
            },
        },
    },
    error::AppError,
    features::{
        proposals::{SetupParamsTable, StatusBadge, new_idempotency_key},
        reauth::ReauthPanel,
        ui::{Empty, ErrorNotice, Loading, action_error},
    },
    i18n::t,
};

#[server(endpoint = "voting/page")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn load_vote_page(id: String) -> Result<VoteData, AppError> {
    use crate::{application::voting, features::proposals::api::srv, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    voting::page(
        &*state.db.read().await?,
        &state.flags,
        state.now(),
        state.tz(),
        !state.config.is_production(),
        viewer.as_ref(),
        &id,
    )
    .await
}

/// Casts a public ballot. Choice: `yes` | `no` | `abstain`. There is no private
/// mode in this build; eligibility, deadline and reauth are checked on the server.
#[server(endpoint = "voting/cast")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id, public_consent = %public_consent), err(level = "debug")))]
pub async fn cast_vote(
    id: String,
    choice: String,
    public_consent: bool,
    idempotency_key: Option<String>,
) -> Result<CastConfirmation, AppError> {
    use crate::{application::voting, features::proposals::api::srv, server::context};
    srv::check_id(&id)?;
    let choice = Choice::from_db(&choice)
        .ok_or_else(|| AppError::validation("choice", t("voting.error.choice")))?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    srv::advance(&state).await?;
    voting::cast(
        &state.proposal_ctx(),
        &actor,
        &id,
        choice,
        public_consent,
        idempotency_key.as_deref(),
    )
    .await
}

/// Own ballot and whether an attempt key produced a stored result (unknown outcome).
#[server(endpoint = "voting/status")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn check_vote_status(
    id: String,
    idempotency_key: Option<String>,
) -> Result<VoteStatus, AppError> {
    use crate::{application::voting, features::proposals::api::srv, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    let actor = context::require_actor().await?;
    voting::status(
        &*state.db.read().await?,
        state.tz(),
        &actor,
        &id,
        idempotency_key.as_deref(),
    )
    .await
}

#[server(endpoint = "voting/result")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn load_vote_result(id: String) -> Result<ResultData, AppError> {
    use crate::{application::voting, features::proposals::api::srv, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    srv::advance(&state).await?;
    voting::result(
        &*state.db.read().await?,
        state.now(),
        state.tz(),
        !state.config.is_production(),
        &id,
    )
    .await
}

#[server(endpoint = "setup/overview")]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn load_setup_overview() -> Result<SetupOverview, AppError> {
    use crate::{application::plebiscite, features::proposals::api::srv, server::context};
    let state = context::state()?;
    let viewer = srv::viewer().await?;
    srv::advance(&state).await?;
    let conn = state.db.read().await?;
    let country: String = crate::infrastructure::db::query_opt(
        &conn,
        "SELECT id FROM jurisdictions WHERE kind = 'national' LIMIT 1",
        (),
    )
    .await?
    .ok_or(AppError::NotFound)?
    .get(0)?;
    plebiscite::overview(&conn, state.tz(), &country, viewer.as_ref()).await
}

/// Country admin starts the national plebiscite with the suggested package.
#[server(endpoint = "setup/start")]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn start_plebiscite() -> Result<String, AppError> {
    use crate::{application::plebiscite, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    plebiscite::start(&state.db, &state.flags, state.now(), &actor).await
}

#[server(endpoint = "setup/diff")]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug")))]
pub async fn load_setup_diff(id: String) -> Result<SetupDiff, AppError> {
    use crate::{application::plebiscite, features::proposals::api::srv, server::context};
    srv::check_id(&id)?;
    let state = context::state()?;
    plebiscite::diff(&*state.db.read().await?, &id).await
}

#[server(endpoint = "statistics")]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn public_statistics() -> Result<Statistics, AppError> {
    use crate::{application::statistics, features::proposals::api::srv, server::context};
    context::require_public_read(true, crate::domain::iam::Permission::StatsRead).await?;
    let state = context::state()?;
    srv::advance(&state).await?;
    statistics::statistics(
        &*state.db.read().await?,
        statistics::process_citizen_counts(),
    )
    .await
}

fn route_id() -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("id").unwrap_or_default())
}

#[component]
fn DemoNotice() -> impl IntoView {
    view! {
        <p class="notice" role="note" data-testid="vote-demo">{t("voting.demo_notice")}</p>
    }
}

// ------------------------------------------------------------------ vote page

#[derive(Clone)]
enum Stage {
    Choose,
    Reauth,
    Sending,
    /// Transport failure: the server may or may not have stored the ballot.
    Unknown,
}

/// `/propostas/:id/votar`.
#[component]
pub fn VotePage() -> impl IntoView {
    let id = route_id();
    let refresh = RwSignal::new(0u32);
    let confirmed = RwSignal::new(None::<CastConfirmation>);
    let data = Resource::new(
        move || (id.get(), refresh.get()),
        |(id, _)| load_vote_page(id),
    );
    view! {
        <section class="page">
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(page) => vote_view(page, refresh, confirmed).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn vote_view(
    page: VoteData,
    refresh: RwSignal<u32>,
    confirmed: RwSignal<Option<CastConfirmation>>,
) -> impl IntoView {
    let s = page.summary.clone();
    let id = s.id.clone();
    let body = match (&page.viewer, page.open) {
        _ if !page.voting_enabled => {
            view! { <p class="notice" role="status">{t("voting.disabled")}</p> }.into_any()
        }
        (Some(VoterState { ballot: Some(b), .. }), _) => view! {
            <p class="notice success" role="status" data-testid="vote-already">
                {t("voting.already")}" "<strong>{t(b.choice.label_key())}</strong>" — "{b.cast_at.clone()}
            </p>
        }
        .into_any(),
        (_, false) => view! {
            <p class="notice" role="status" data-testid="vote-closed">{t("voting.closed")}</p>
        }
        .into_any(),
        (None, true) => view! {
            <p class="notice" role="status">{t("voting.login")}" "<A href="/login">{t("nav.login")}</A></p>
        }
        .into_any(),
        (Some(v), true) if !v.eligible => view! {
            <p class="notice" role="status" data-testid="vote-ineligible">{t("voting.ineligible")}</p>
        }
        .into_any(),
        (Some(_), true) => view! {
            <VoteForm id=id.clone() action=page.reauth_action.clone() refresh confirmed/>
        }
        .into_any(),
    };
    view! {
        <h1 data-testid="proposal-title">{s.title.clone()}</h1>
        <p class="row">
            <StatusBadge status=s.status/>
            <span class="badge">{t(s.kind.label_key())}</span>
            <span class="badge">{s.jurisdiction.clone()}</span>
        </p>
        <DemoNotice/>
        {page.ends_at.clone().map(|at| view! { <p>{t("proposals.deadline.vote")}": "{at}" "<span class="hint">{t("proposals.timezone_note")}</span></p> })}
        {page.electorate.map(|n| view! { <p>{t("voting.electorate")}": "{n}</p> })}
        <div aria-live="polite">
            {move || confirmed.get().map(|c| view! {
                <p class="notice success" role="status" data-testid="vote-confirmed">
                    {t("voting.confirmed")}" "<strong>{t(c.choice.label_key())}</strong>" — "{c.cast_at}
                </p>
            })}
        </div>
        {body}
        <p class="row">
            <A href=format!("/propostas/{id}")>{t("proposals.back")}</A>
            <A href=format!("/propostas/{id}/resultado")>{t("voting.result.link")}</A>
        </p>
    }
}

#[component]
fn VoteForm(
    id: String,
    action: String,
    refresh: RwSignal<u32>,
    confirmed: RwSignal<Option<CastConfirmation>>,
) -> impl IntoView {
    let choice = RwSignal::new(None::<Choice>);
    let consent = RwSignal::new(false);
    let stage = RwSignal::new(Stage::Choose);
    let key = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<AppError>);
    let id = StoredValue::new(id);
    let action = StoredValue::new(action);

    let start = move |_| {
        error.set(None);
        if choice.get_untracked().is_none() {
            error.set(Some(AppError::validation(
                "choice",
                t("voting.error.choice"),
            )));
            return;
        }
        if !consent.get_untracked() {
            error.set(Some(AppError::validation(
                "public_consent",
                t("voting.error.consent"),
            )));
            return;
        }
        // One key per attempt: a retry after an unknown outcome reuses it.
        key.set(new_idempotency_key());
        stage.set(Stage::Reauth);
    };
    let send = move || {
        let Some(c) = choice.get_untracked() else {
            return;
        };
        stage.set(Stage::Sending);
        let (pid, k) = (id.get_value(), key.get_untracked());
        spawn_local(async move {
            match cast_vote(pid, c.as_db().to_owned(), true, k).await {
                Ok(done) => {
                    confirmed.set(Some(done));
                    refresh.update(|n| *n += 1);
                }
                Err(AppError::Transport(_)) => stage.set(Stage::Unknown),
                Err(e @ AppError::Conflict(_)) | Err(e @ AppError::PhaseClosed) => {
                    error.set(Some(e));
                    refresh.update(|n| *n += 1);
                }
                Err(e) => {
                    error.set(Some(e));
                    stage.set(Stage::Choose);
                }
            }
        });
    };
    let check = move |_| {
        let (pid, k) = (id.get_value(), key.get_untracked());
        spawn_local(async move {
            match check_vote_status(pid.clone(), k).await {
                Ok(VoteStatus {
                    ballot: Some(b), ..
                }) => {
                    confirmed.set(Some(CastConfirmation {
                        proposal_id: pid,
                        choice: b.choice,
                        cast_at: b.cast_at,
                    }));
                    refresh.update(|n| *n += 1);
                }
                Ok(_) => {
                    error.set(Some(AppError::Conflict(t("voting.not_recorded").into())));
                    stage.set(Stage::Choose);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <section class="card" aria-labelledby="vote-title">
            <h2 id="vote-title">{t("voting.title")}</h2>
            <fieldset>
                <legend>{t("voting.choice.legend")}</legend>
                {Choice::ALL.into_iter().map(|c| view! {
                    <label class="row">
                        <input type="radio" name="choice" value=c.as_db()
                            data-testid=format!("vote-choice-{}", c.as_db())
                            prop:checked=move || choice.get() == Some(c)
                            disabled=move || !matches!(stage.get(), Stage::Choose)
                            on:change=move |_| choice.set(Some(c))/>
                        {t(c.label_key())}
                    </label>
                }).collect_view()}
            </fieldset>
            <fieldset>
                <legend>{t("voting.privacy.legend")}</legend>
                <label class="row">
                    <input type="radio" name="visibility" disabled/>
                    {t("voting.privacy.private")}
                </label>
                <p class="hint" data-testid="vote-private-unavailable">{t("voting.privacy.private_unavailable")}</p>
                <label class="row">
                    <input type="checkbox" data-testid="vote-consent"
                        prop:checked=move || consent.get()
                        disabled=move || !matches!(stage.get(), Stage::Choose)
                        on:change=move |ev| consent.set(event_target_checked(&ev))/>
                    {t("voting.privacy.consent")}
                </label>
            </fieldset>
            {move || match stage.get() {
                Stage::Choose => view! {
                    <button type="button" data-testid="vote-continue" on:click=start>{t("voting.continue")}</button>
                }.into_any(),
                Stage::Reauth => {
                    let on_granted = Callback::new(move |_| send());
                    view! {
                        <p class="hint">{t("voting.reauth_hint")}</p>
                        <ReauthPanel action=action.get_value() resource=id.get_value() on_granted/>
                        <button type="button" class="secondary" on:click=move |_| stage.set(Stage::Choose)>{t("common.cancel")}</button>
                    }.into_any()
                }
                Stage::Sending => view! {
                    <p class="notice" role="status" data-testid="vote-sending">{t("voting.sending")}</p>
                }.into_any(),
                Stage::Unknown => view! {
                    <p class="notice error" role="alert" data-testid="vote-unknown">{t("voting.unknown")}</p>
                    <button type="button" data-testid="vote-check" on:click=check>{t("voting.check")}</button>
                }.into_any(),
            }}
            {move || error.get().map(|error| view! { <ErrorNotice error/> })}
        </section>
    }
}

// ---------------------------------------------------------------- result page

fn reason_key(o: Outcome) -> Option<&'static str> {
    match o {
        Outcome::Approved => None,
        Outcome::QuorumNotReached => Some("voting.reason.quorum"),
        Outcome::Rejected(RejectReason::Tie) => Some("voting.reason.tie"),
        Outcome::Rejected(RejectReason::NoValidVotes) => Some("voting.reason.no_valid"),
        Outcome::Rejected(RejectReason::MajorityNotReached) => Some("voting.reason.majority"),
    }
}

/// `/propostas/:id/resultado`.
#[component]
pub fn ResultPage() -> impl IntoView {
    let id = route_id();
    let data = Resource::new(move || id.get(), load_vote_result);
    view! {
        <section class="page">
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(page) => result_view(page).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn result_view(page: ResultData) -> impl IntoView {
    let s = page.summary.clone();
    let id = s.id.clone();
    let kind = s.kind;
    let state = match page.state {
        ResultState::NotOpened => view! {
            <p class="notice" role="status" data-testid="result-not-opened">{t("voting.result.not_opened")}</p>
        }
        .into_any(),
        ResultState::Open { ends_at, electorate } => view! {
            <p class="notice" role="status" data-testid="result-open">
                {t("voting.result.open")}" "{ends_at}". "{t("voting.electorate")}": "{electorate}"."
            </p>
            <p class="hint">{t("voting.result.no_partials")}</p>
            <p><A href=format!("/propostas/{id}/votar") attr:class="button">{t("voting.vote_link")}</A></p>
        }
        .into_any(),
        ResultState::Closing => view! {
            <p class="notice" role="status" data-testid="result-closing">{t("voting.result.closing")}</p>
        }
        .into_any(),
        ResultState::Closed(r) => closed_view(r, kind).into_any(),
    };
    view! {
        <h1 data-testid="proposal-title">{s.title.clone()}</h1>
        <p class="row">
            <StatusBadge status=s.status/>
            <span class="badge">{t(s.kind.label_key())}</span>
            <span class="badge">{s.jurisdiction.clone()}</span>
        </p>
        <DemoNotice/>
        {state}
        <p><A href=format!("/propostas/{}", s.id)>{t("proposals.back")}</A></p>
    }
}

fn closed_view(r: ResultView, kind: ProposalKind) -> impl IntoView {
    let tally = r.tally;
    let participation = tally.participation();
    let valid = tally.valid();
    let quorum_ok = participation >= r.quorum_required;
    let package = r.package.map(|adopted| match adopted {
        Some(v) => format!("{} {v}", t("voting.result.package_adopted")),
        None if kind == ProposalKind::InitialSetup => {
            t("voting.result.package_rejected_initial").into()
        }
        None => t("voting.result.package_rejected").into(),
    });
    view! {
        <p class="lead" data-testid="result-outcome">
            <strong>{t(r.outcome.status().label_key())}</strong>
            {reason_key(r.outcome).map(|k| format!(" — {}", t(k)))}
        </p>
        {package.map(|p| view! { <p class="notice" data-testid="result-package">{p}</p> })}
        <div class="table-scroll">
            <table>
                <thead><tr><th>{t("voting.result.option")}</th><th>{t("voting.result.votes")}</th><th>{t("voting.result.share")}</th></tr></thead>
                <tbody>
                    <tr><th scope="row">{t("voting.choice.yes")}</th><td data-testid="result-yes">{tally.yes}</td><td>{format_share(tally.yes, valid)}" "{t("voting.result.of_valid")}</td></tr>
                    <tr><th scope="row">{t("voting.choice.no")}</th><td data-testid="result-no">{tally.no}</td><td>{format_share(tally.no, valid)}" "{t("voting.result.of_valid")}</td></tr>
                    <tr><th scope="row">{t("voting.choice.abstain")}</th><td data-testid="result-abstain">{tally.abstain}</td><td>{format_share(tally.abstain, participation)}" "{t("voting.result.of_participation")}</td></tr>
                </tbody>
            </table>
        </div>
        <dl class="facts">
            <dt>{t("voting.electorate")}</dt><dd data-testid="result-electorate">{r.electorate}</dd>
            <dt>{t("voting.result.snapshot")}</dt><dd><code>{r.snapshot_id.clone()}</code></dd>
            <dt>{t("voting.result.participation")}</dt>
            <dd>{participation}" ("{format_share(participation, r.electorate)}" "{t("voting.result.of_electorate")}")"</dd>
            <dt>{t("voting.result.quorum")}</dt>
            <dd data-testid="result-quorum">
                {format!("{} {} ({} × {})", r.quorum_required, t("voting.result.votes").to_lowercase(), format_bp(r.quorum_bp), r.electorate)}
                " — "{if quorum_ok { t("voting.result.reached") } else { t("voting.result.not_reached") }}
            </dd>
            <dt>{t("voting.result.valid")}</dt><dd>{valid}</dd>
            <dt>{t("voting.result.majority")}</dt>
            <dd>{format!("{} {} (> {} {})", r.majority_required, t("voting.choice.yes"), format_bp(r.majority_bp), t("voting.result.of_valid"))}</dd>
            <dt>{t("proposals.field.setup_version")}</dt>
            <dd>{r.setup_version.map(|v| v.to_string()).unwrap_or_else(|| "—".into())}" "<code class="hint">{r.setup_version_id.clone()}</code></dd>
            <dt>{t("voting.result.closed_at")}</dt><dd>{r.closed_at.clone()}</dd>
        </dl>
        <p class="hint">{t("voting.result.rounding")}</p>
    }
}

// -------------------------------------------------------- initial plebiscite

/// `/setup`: initial plebiscite state; the country admin may start it with the
/// suggested package, shown read-only.
#[component]
pub fn SetupPage() -> impl IntoView {
    let start = ServerAction::<StartPlebiscite>::new();
    let data = Resource::new(move || start.version().get(), |_| load_setup_overview());
    view! {
        <section class="page">
            <h1>{t("voting.setup.title")}</h1>
            <DemoNotice/>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(o) => setup_view(o, start).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
            {move || action_error(start.value().get())}
            <p><A href="/setup/historico">{t("setup.history.title")}</A></p>
        </section>
    }
}

fn setup_view(o: SetupOverview, start: ServerAction<StartPlebiscite>) -> impl IntoView {
    let state = match (&o.effective_version, &o.open_plebiscite) {
        (Some(v), _) => format!(
            "{} {v} ({})",
            t("voting.setup.adopted"),
            o.effective_adopted_at.clone().unwrap_or_default()
        ),
        (None, Some(_)) => t("voting.setup.open").into(),
        (None, None) if o.past_plebiscites.is_empty() => t("voting.setup.not_started").into(),
        (None, None) => t("voting.setup.failed").into(),
    };
    view! {
        <p class="notice" role="status" data-testid="setup-state">{state}</p>
        {o.open_plebiscite.clone().map(|p| view! {
            <div class="card" data-testid="setup-open">
                <h2>{p.title.clone()}</h2>
                <p>{t("proposals.deadline.vote")}": "{p.deadline.clone().unwrap_or_default()}</p>
                <p class="row">
                    <A href=format!("/propostas/{}/votar", p.id) attr:class="button">{t("voting.vote_link")}</A>
                    <A href=format!("/propostas/{}/resultado", p.id)>{t("voting.result.link")}</A>
                </p>
            </div>
        })}
        {o.can_start.then(|| view! {
            <ActionForm action=start>
                <p class="hint">{t("voting.setup.start_hint")}</p>
                <button type="submit" data-testid="setup-start" disabled=move || start.pending().get()>{t("voting.setup.start")}</button>
            </ActionForm>
        })}
        <h2>{t("voting.setup.suggested")}</h2>
        <p class="hint">{t("voting.setup.readonly")}</p>
        <SetupParamsTable params=o.suggested/>
        {(!o.past_plebiscites.is_empty()).then(move || view! {
            <h2>{t("voting.setup.past")}</h2>
            <ul class="list">
                {o.past_plebiscites.into_iter().map(|p| view! {
                    <li class="card">
                        <A href=format!("/propostas/{}/resultado", p.id)>{p.title.clone()}</A>" "<StatusBadge status=p.status/>
                    </li>
                }).collect_view()}
            </ul>
        })}
    }
}

/// Current vs proposed setup parameters (meta-law detail).
#[component]
pub fn SetupDiffCard(id: Memo<String>) -> impl IntoView {
    let data = Resource::new(move || id.get(), load_setup_diff);
    view! {
        <Transition fallback=|| ()>
            {move || data.get().map(|r| match r {
                Ok(d) => diff_view(d).into_any(),
                Err(error) => view! { <ErrorNotice error/> }.into_any(),
            })}
        </Transition>
    }
}

fn diff_view(d: SetupDiff) -> impl IntoView {
    let current = d.current.map(|c| c.fields());
    view! {
        <section class="card" aria-labelledby="diff-title" data-testid="setup-diff">
            <h2 id="diff-title">{t("voting.diff.title")}" — "{d.jurisdiction.clone()}</h2>
            <p class="hint">{match d.current_version {
                Some(v) => format!("{} {v}", t("voting.diff.current_version")),
                None => t("voting.diff.no_current").into(),
            }}</p>
            <div class="table-scroll">
                <table>
                    <thead><tr><th>{t("voting.diff.param")}</th><th>{t("voting.diff.current")}</th><th>{t("voting.diff.proposed")}</th></tr></thead>
                    <tbody>
                        {d.proposed.fields().into_iter().enumerate().map(|(i, (name, value))| {
                            let cur = current.map(|c| c[i].1);
                            let changed = cur.is_some_and(|c| c != value);
                            view! {
                                <tr data-changed=changed.then_some("true")>
                                    <th scope="row">{t(name)}</th>
                                    <td>{cur.map(|c| c.to_string()).unwrap_or_else(|| "—".into())}</td>
                                    <td>{if changed { format!("{value} ●") } else { value.to_string() }}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <p class="hint">{t("voting.diff.note")}</p>
        </section>
    }
}

// ---------------------------------------------------------------- statistics

/// `/estatisticas`: public aggregates only.
#[component]
pub fn StatisticsPage() -> impl IntoView {
    let data = Resource::new(|| (), |_| public_statistics());
    view! {
        <section class="page">
            <h1>{t("stats.title")}</h1>
            <DemoNotice/>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(s) => stats_view(s).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn stats_view(s: Statistics) -> impl IntoView {
    view! {
        <h2>{t("stats.by_status")}</h2>
        {if s.by_status.is_empty() { view! { <Empty/> }.into_any() } else { view! {
            <table data-testid="stats-status"><tbody>
                {s.by_status.iter().map(|(st, n)| view! { <tr><th scope="row">{t(st.label_key())}</th><td>{*n}</td></tr> }).collect_view()}
            </tbody></table>
        }.into_any() }}
        <h2>{t("stats.by_kind")}</h2>
        <table><tbody>
            {s.by_kind.iter().map(|(k, n)| view! { <tr><th scope="row">{t(k.label_key())}</th><td>{*n}</td></tr> }).collect_view()}
        </tbody></table>
        <h2>{t("stats.closed_votes")}</h2>
        {if s.closed_votes.is_empty() { view! { <Empty/> }.into_any() } else { view! {
            <div class="table-scroll"><table data-testid="stats-votes">
                <thead><tr><th>{t("proposals.field.title")}</th><th>{t("proposals.field.status")}</th><th>{t("voting.result.participation")}</th></tr></thead>
                <tbody>
                    {s.closed_votes.clone().into_iter().map(|v| view! {
                        <tr>
                            <td><A href=format!("/propostas/{}/resultado", v.id)>{v.title.clone()}</A></td>
                            <td>{t(v.status.label_key())}</td>
                            <td>{format!("{} / {} ({})", v.participation, v.electorate, format_share(v.participation, v.electorate))}</td>
                        </tr>
                    }).collect_view()}
                </tbody>
            </table></div>
        }.into_any() }}
        <h2>{t("stats.citizens")}</h2>
        <p class="hint">{format!("{} {}.", t("stats.min_cell"), s.min_cell)}</p>
        <table data-testid="stats-citizens"><tbody>
            {s.citizens_by_state.iter().map(|c| view! {
                <tr><th scope="row">{c.label.clone()}</th>
                    <td>{c.count.map(|n| n.to_string()).unwrap_or_else(|| format!("< {}", s.min_cell))}</td></tr>
            }).collect_view()}
            <tr><th scope="row">{t("stats.total")}</th>
                <td>{s.citizens_total.map(|n| n.to_string()).unwrap_or_else(|| t("stats.suppressed").into())}</td></tr>
        </tbody></table>
    }
}
