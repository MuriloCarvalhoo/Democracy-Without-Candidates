//! Administration panel: feature flags (stored vs effective, pending restart) and
//! instance health. Accreditation is IAM (`features::iam`). The server authorizes
//! every call; the panel never restarts the process.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::{
    domain::admin::{AdminOverview, FlagView, InstanceStats},
    error::AppError,
    features::ui::{Loading, action_error},
    i18n::t,
};

#[server]
pub async fn load_admin_overview() -> Result<AdminOverview, AppError> {
    use crate::{application::admin, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    admin::overview(&state.db, &actor, &state.flags, &state.config.db_path).await
}

#[server]
pub async fn set_feature_flag(key: String, enabled: bool) -> Result<(), AppError> {
    use crate::{application::admin, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    admin::set_flag(&state.db, state.now(), &actor, &key, enabled).await
}

#[component]
pub(crate) fn AdminNav() -> impl IntoView {
    view! {
        <nav class="row" aria-label=t("admin.nav")>
            <A href="/admin" exact=true>{t("admin.nav.instance")}</A>
            <A href="/admin/iam">{t("admin.nav.iam")}</A>
            <A href="/admin/moderacao">{t("debate.queue.nav")}</A>
        </nav>
    }
}

#[component]
pub(crate) fn Denied(error: AppError) -> impl IntoView {
    view! { <p class="notice error" role="alert" data-testid="admin-denied">{error.user_message()}</p> }
}

fn on_off(on: bool) -> &'static str {
    if on {
        t("admin.flag.on")
    } else {
        t("admin.flag.off")
    }
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let toggle = ServerAction::<SetFeatureFlag>::new();
    let overview = Resource::new(move || toggle.version().get(), |_| load_admin_overview());

    view! {
        <section class="page">
            <h1>{t("admin.title")}</h1>
            <AdminNav/>
            <Transition fallback=|| view! { <Loading/> }>
                {move || overview.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(o) => view! {
                        <FlagsCard flags=o.flags toggle/>
                        <StatsCard stats=o.stats/>
                    }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[component]
fn FlagsCard(flags: Vec<FlagView>, toggle: ServerAction<SetFeatureFlag>) -> impl IntoView {
    let any_pending = flags.iter().any(|f| f.pending_restart);
    let rows = flags
        .into_iter()
        .map(|f| {
            let stored_on = f.stored.unwrap_or(false);
            let status = if !f.implemented {
                Some(view! { <span class="badge" data-testid="flag-not-implemented">{t("admin.flag.not_implemented")}</span> })
            } else if f.pending_restart {
                Some(view! { <span class="badge" data-testid="flag-pending">{t("admin.flag.pending")}</span> })
            } else {
                None
            };
            let can_toggle = f.implemented || stored_on;
            let key = f.key.clone();
            let label = if stored_on { t("admin.flag.disable") } else { t("admin.flag.enable") };
            view! {
                <tr data-testid=format!("flag-{}", f.key)>
                    <td><code>{f.key.clone()}</code><br/><span class="hint">{f.description}</span></td>
                    <td data-testid="flag-effective">{on_off(f.effective)}</td>
                    <td data-testid="flag-stored">{on_off(stored_on)}</td>
                    <td>{status}</td>
                    <td>
                        {can_toggle.then(|| view! {
                            <ActionForm action=toggle>
                                <input type="hidden" name="key" value=key/>
                                <input type="hidden" name="enabled" value=(!stored_on).to_string()/>
                                <button type="submit" class="secondary" data-testid="flag-toggle"
                                    disabled=move || toggle.pending().get()>{label}</button>
                            </ActionForm>
                        })}
                    </td>
                </tr>
            }
        })
        .collect_view();

    view! {
        <section class="card" aria-labelledby="flags-title">
            <h2 id="flags-title">{t("admin.flags.title")}</h2>
            <p class="hint">{t("admin.flags.hint")}</p>
            {any_pending.then(|| view! {
                <p class="notice" role="status" data-testid="restart-pending">{t("admin.flags.restart_pending")}</p>
            })}
            {move || action_error(toggle.value().get())}
            <div class="table-scroll">
                <table>
                    <thead><tr>
                        <th scope="col">{t("admin.flags.col.flag")}</th>
                        <th scope="col">{t("admin.flags.col.effective")}</th>
                        <th scope="col">{t("admin.flags.col.stored")}</th>
                        <th scope="col">{t("admin.flags.col.status")}</th>
                        <th scope="col"><span class="visually-hidden">{t("admin.flags.col.action")}</span></th>
                    </tr></thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
}

fn human_bytes(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b => format!("{:.1} KiB", b as f64 / 1024.0),
    }
}

#[component]
fn StatsCard(stats: InstanceStats) -> impl IntoView {
    let users = if stats.users_by_status.is_empty() {
        t("common.empty").to_string()
    } else {
        stats
            .users_by_status
            .iter()
            .map(|(s, n)| format!("{s}: {n}"))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let catalogs = stats
        .catalogs
        .iter()
        .map(|(c, sum, at)| format!("{c} ({sum}…, {at})"))
        .collect::<Vec<_>>()
        .join("; ");
    let flags = if stats.effective_flags.is_empty() {
        t("admin.flag.none").to_string()
    } else {
        stats.effective_flags.join(", ")
    };
    view! {
        <section class="card" aria-labelledby="stats-title" data-testid="instance-stats">
            <h2 id="stats-title">{t("admin.stats.title")}</h2>
            <dl class="facts">
                <dt>{t("admin.stats.users")}</dt><dd>{users}</dd>
                <dt>{t("admin.stats.db_size")}</dt><dd>{human_bytes(stats.db_bytes)}</dd>
                <dt>{t("admin.stats.migrations")}</dt><dd>{stats.migrations.join(", ")}</dd>
                <dt>{t("admin.stats.catalog")}</dt><dd>{catalogs}</dd>
                <dt>{t("admin.stats.effective")}</dt><dd>{flags}</dd>
            </dl>
        </section>
    }
}
