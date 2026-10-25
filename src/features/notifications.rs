//! Notification center, unread badge and per-category preferences (in-app only;
//! email is not available in this instance).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::{
    domain::notification::{NotificationPage, Preference},
    error::AppError,
    features::{
        audit::format_instant,
        ui::{Empty, ErrorNotice, Loading, action_error},
    },
    i18n::t,
};

/// Unread count for the nav badge; `None` without a valid session. Background
/// fetch: does not renew session activity.
#[server]
pub async fn unread_notification_count() -> Result<Option<i64>, AppError> {
    use crate::{application::notify, server::context};
    let actor = match context::current_actor(false).await {
        Ok(Some(actor)) => actor,
        Ok(None) | Err(AppError::SessionExpired) => return Ok(None),
        Err(e) => return Err(e),
    };
    let state = context::state()?;
    Ok(Some(
        notify::unread_count(&state.db.read().await?, &actor.user_id).await?,
    ))
}

#[server]
pub async fn notification_list(page: u32) -> Result<NotificationPage, AppError> {
    use crate::{application::notify, server::context};
    let actor = context::require_actor().await?;
    let state = context::state()?;
    notify::list(&state.db.read().await?, &actor.user_id, page).await
}

#[server]
pub async fn mark_notification_read(id: String) -> Result<(), AppError> {
    use crate::{application::notify, server::context};
    let actor = context::require_actor().await?;
    let state = context::state()?;
    let tx = state.db.write().await?;
    notify::mark_read(&tx, state.now(), &actor.user_id, &id).await?;
    tx.commit().await?;
    Ok(())
}

#[server]
pub async fn mark_all_notifications_read() -> Result<u64, AppError> {
    use crate::{application::notify, server::context};
    let actor = context::require_actor().await?;
    let state = context::state()?;
    let tx = state.db.write().await?;
    let changed = notify::mark_all_read(&tx, state.now(), &actor.user_id).await?;
    tx.commit().await?;
    Ok(changed)
}

#[server]
pub async fn notification_settings() -> Result<Vec<Preference>, AppError> {
    use crate::{application::notify, server::context};
    let actor = context::require_actor().await?;
    let state = context::state()?;
    notify::preferences(&state.db.read().await?, &actor.user_id).await
}

#[server]
pub async fn set_notification_preference(category: String, in_app: bool) -> Result<(), AppError> {
    use crate::{application::notify, domain::notification::Category, server::context};
    let category = Category::from_db(&category)
        .ok_or_else(|| AppError::validation("category", "categoria desconhecida"))?;
    let actor = context::require_actor().await?;
    let state = context::state()?;
    let tx = state.db.write().await?;
    notify::set_preference(&tx, state.now(), &actor.user_id, category, in_app).await?;
    tx.commit().await?;
    Ok(())
}

/// Bumped after read-state changes so the nav badge refetches.
#[derive(Clone, Copy)]
pub struct NotificationsRefresh(pub RwSignal<u32>);

pub fn provide_notifications_refresh() {
    provide_context(NotificationsRefresh(RwSignal::new(0)));
}

fn bump_refresh() {
    if let Some(NotificationsRefresh(v)) = use_context::<NotificationsRefresh>() {
        v.update(|n| *n += 1);
    }
}

/// Nav link with unread count; rendered only for signed-in viewers.
#[component]
pub fn NotificationsBadge() -> impl IntoView {
    let refresh = use_context::<NotificationsRefresh>().map(|r| r.0);
    let tick = RwSignal::new(0u32);
    #[cfg(feature = "hydrate")]
    set_interval(
        move || tick.update(|n| *n += 1),
        std::time::Duration::from_secs(60),
    );
    let count = Resource::new(
        move || (refresh.map(|r| r.get()), tick.get()),
        |_| unread_notification_count(),
    );
    view! {
        <Transition fallback=|| ()>
            {move || count.get().and_then(|r| r.ok()).flatten().map(|n| view! {
                <A href="/notificacoes" attr:class="nav-notifications" attr:data-testid="nav-notifications">
                    <svg class="icon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
                        <path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15zM10 20.5a2 2 0 0 0 4 0" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>
                    </svg>
                    <span class="nav-label">{t("nav.notifications")}</span>
                    {(n > 0).then(|| view! {
                        " "<span class="badge" data-testid="nav-unread">
                            {n}<span class="visually-hidden">" "{t("notifications.unread")}</span>
                        </span>
                    })}
                </A>
            })}
        </Transition>
    }
}

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let page = RwSignal::new(0u32);
    let mark = ServerAction::<MarkNotificationRead>::new();
    let mark_all = ServerAction::<MarkAllNotificationsRead>::new();
    let data = Resource::new(
        move || (page.get(), mark.version().get(), mark_all.version().get()),
        |(p, _, _)| notification_list(p),
    );
    Effect::new(move |prev: Option<(usize, usize)>| {
        let now = (mark.version().get(), mark_all.version().get());
        if prev.is_some_and(|p| p != now) {
            bump_refresh();
        }
        now
    });
    view! {
        <section class="page">
            <h1>{t("notifications.title")}</h1>
            <A href="/perfil/notificacoes">{t("notifications.prefs.link")}</A>
        </section>
        {move || action_error(mark.value().get())}
        {move || action_error(mark_all.value().get())}
        <Transition fallback=|| view! { <Loading/> }>
            {move || data.get().map(|r| match r {
                Err(error) => view! { <ErrorNotice error/> }.into_any(),
                Ok(NotificationPage { items, unread, has_more }) => view! {
                    <div class="row">
                        <p data-testid="notifications-unread">{unread}" "{t("notifications.unread")}</p>
                        <ActionForm action=mark_all>
                            <button type="submit" class="secondary" disabled=unread == 0>
                                {t("notifications.mark_all")}
                            </button>
                        </ActionForm>
                    </div>
                    {if items.is_empty() {
                        view! { <Empty message=t("notifications.empty")/> }.into_any()
                    } else {
                        view! {
                            <ul class="list" data-testid="notifications-list">
                                {items.into_iter().map(|n| view! {
                                    <li class="card" data-read=n.read.to_string()>
                                        <strong>{n.title.clone()}</strong>
                                        <p>{n.body.clone()}</p>
                                        // <div>, not <p>: a <form> inside <p> is invalid HTML and breaks hydration.
                                        <div class="row">
                                            <small>{format_instant(&n.created_at)}</small>
                                            {n.link.clone().map(|href| view! { <A href=href>{t("notifications.open")}</A> })}
                                            {if n.read {
                                                view! { <span class="badge">{t("notifications.read")}</span> }.into_any()
                                            } else {
                                                view! {
                                                    <ActionForm action=mark>
                                                        <input type="hidden" name="id" value=n.id.clone()/>
                                                        <button type="submit" class="secondary">{t("notifications.mark_read")}</button>
                                                    </ActionForm>
                                                }.into_any()
                                            }}
                                        </div>
                                    </li>
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }}
                    <p class="row">
                        <Show when=move || page.get() != 0>
                            <button class="secondary" on:click=move |_| page.update(|p| *p -= 1)>
                                {t("notifications.newer")}
                            </button>
                        </Show>
                        {has_more.then(|| view! {
                            <button class="secondary" on:click=move |_| page.update(|p| *p += 1)>
                                {t("notifications.older")}
                            </button>
                        })}
                    </p>
                }.into_any(),
            })}
        </Transition>
    }
}

#[component]
pub fn NotificationPreferencesPage() -> impl IntoView {
    let set = ServerAction::<SetNotificationPreference>::new();
    let prefs = Resource::new(move || set.version().get(), |_| notification_settings());
    view! {
        <section class="page">
            <h1>{t("notifications.prefs.title")}</h1>
            <p class="lead">{t("notifications.prefs.lead")}</p>
            <A href="/notificacoes">{t("notifications.title")}</A>
        </section>
        {move || action_error(set.value().get())}
        <Transition fallback=|| view! { <Loading/> }>
            {move || prefs.get().map(|r| match r {
                Err(error) => view! { <ErrorNotice error/> }.into_any(),
                Ok(prefs) => view! {
                    <section class="card">
                        <table>
                            <thead><tr>
                                <th>{t("notifications.prefs.category")}</th>
                                <th>{t("notifications.prefs.in_app")}</th>
                                <th>{t("notifications.prefs.email")}</th>
                            </tr></thead>
                            <tbody>
                                {prefs.into_iter().map(|p| {
                                    let category = p.category;
                                    view! {
                                        <tr>
                                            <td>{t(category.label_key())}</td>
                                            <td>
                                                <input
                                                    type="checkbox"
                                                    aria-label=t(category.label_key())
                                                    data-testid=format!("pref-{}", category.as_db())
                                                    checked=p.in_app
                                                    disabled=move || set.pending().get()
                                                    on:change=move |ev| {
                                                        set.dispatch(SetNotificationPreference {
                                                            category: category.as_db().to_owned(),
                                                            in_app: event_target_checked(&ev),
                                                        });
                                                    }
                                                />
                                            </td>
                                            <td><span class="hint">{t("notifications.prefs.email_unavailable")}</span></td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                        <p class="hint">{t("notifications.prefs.email_note")}</p>
                    </section>
                }.into_any(),
            })}
        </Transition>
    }
}
