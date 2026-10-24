//! Landing page and the foundation probe: a synthetic counter persisted in TursoDB
//! and changed through a server function after hydration.

use leptos::prelude::*;

use crate::{error::AppError, features::ui::action_error, i18n::t};

#[server]
pub async fn get_counter() -> Result<i64, AppError> {
    use crate::{infrastructure::db::query_opt, server::context};
    let state = context::state()?;
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

#[server]
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
        .ok_or(AppError::Internal)?
        .get(0)?;
    tx.commit().await?;
    Ok(value)
}

#[component]
pub fn HomePage() -> impl IntoView {
    let increment = ServerAction::<IncrementCounter>::new();
    let counter = Resource::new(move || increment.version().get(), |_| get_counter());

    view! {
        <section class="page hero">
            <h1>{t("home.title")}</h1>
            <p class="lead">{t("home.lead")}</p>
        </section>
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
    }
}
