//! Profile (`/perfil`): name, private read-only address, participation placeholder;
//! privacy settings (`/perfil/privacidade`), a critical action that needs
//! reauthentication (`profile:privacy`).

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};

use crate::{
    domain::identity::Visibility,
    error::AppError,
    features::{
        reauth::ReauthPanel,
        register::or_dash,
        ui::{ErrorNotice, error_view},
    },
    i18n::t,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileView {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub pending: bool,
    pub totp_reenroll_required: bool,
    pub cep: String,
    pub street: String,
    pub neighborhood: String,
    pub municipality: String,
    pub state: String,
    pub country: String,
    pub profile_visibility: Visibility,
    pub default_vote_visibility: Visibility,
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn my_profile() -> Result<ProfileView, AppError> {
    use crate::{
        application::identity::{enroll, profile},
        domain::identity::{AccountStatus, Permission},
        server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    actor.require(Permission::ProfileRead)?;
    let conn = state.db.read().await?;
    let p = profile::load(&conn, &actor.user_id).await?;
    let s = enroll::status(&state.identity_deps(), &actor.user_id).await?;
    Ok(ProfileView {
        user_id: actor.user_id.clone(),
        name: p.name,
        email: p.email,
        pending: actor.status == AccountStatus::Pending,
        totp_reenroll_required: s.totp_reenroll_required,
        cep: p.address.cep,
        street: p.address.street,
        neighborhood: p.address.neighborhood,
        municipality: p.municipality,
        state: p.address.state,
        country: p.country,
        profile_visibility: p.profile_visibility,
        default_vote_visibility: p.default_vote_visibility,
    })
}

/// Only the name is editable; the address is never changed by any endpoint.
#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn update_profile_name(name: String) -> Result<(), AppError> {
    use crate::{
        application::identity::profile::{self, ProfileUpdate},
        features::face::bounded,
        server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let tx = state.db.write().await?;
    profile::update(
        &tx,
        state.now(),
        &actor,
        ProfileUpdate {
            name: Some(bounded(&name, "name", 512)?.to_owned()),
            cep: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(profile_visibility = ?profile_visibility, default_vote_visibility = ?default_vote_visibility), err(level = "debug")))]
pub async fn save_privacy(
    profile_visibility: Visibility,
    default_vote_visibility: Visibility,
) -> Result<(), AppError> {
    use crate::{application::identity::profile, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let tx = state.db.write().await?;
    profile::set_privacy(
        &tx,
        state.now(),
        &actor,
        profile_visibility,
        default_vote_visibility,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

fn visibility_label(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => t("profile.visibility.public"),
        Visibility::Private => t("profile.visibility.private"),
    }
}

#[component]
pub fn ProfilePage() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let profile = Resource::new(move || refresh.get(), |_| my_profile());
    view! {
        <section class="page">
            <h1>{t("profile.title")}</h1>
            <Transition fallback=|| view! { <p>{t("common.loading")}</p> }>
                {move || profile.get().map(|p| match p {
                    Ok(p) => view! { <ProfileDetails p refresh/> }.into_any(),
                    Err(error) => view! {
                        <ErrorNotice error/>
                        <p><a href="/login">{t("nav.login")}</a></p>
                    }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[component]
fn ProfileDetails(p: ProfileView, refresh: RwSignal<u32>) -> impl IntoView {
    let name = RwSignal::new(p.name.clone());
    let saved = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);
    let save_name = move |ev: SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        saved.set(false);
        let value = name.get_untracked();
        spawn_local(async move {
            match update_profile_name(value).await {
                Ok(()) => {
                    saved.set(true);
                    refresh.update(|r| *r += 1);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        {p.pending.then(|| view! {
            <p class="notice" role="status">{t("profile.pending")}" "<a href="/cadastro/biometria">{t("profile.finish_enrollment")}</a></p>
        })}
        {p.totp_reenroll_required.then(|| view! {
            <p class="notice error" role="alert">{t("profile.reenroll_totp")}" "<a href="/cadastro/biometria">{t("profile.reenroll_link")}</a></p>
        })}
        <section class="card" aria-labelledby="profile-data">
            <h2 id="profile-data">{t("profile.data")}</h2>
            <p data-testid="profile-name">{p.name.clone()}</p>
            <p>{p.email.clone()}</p>
            <form on:submit=save_name class="row">
                <label>{t("auth.field.name")}
                    <input type="text" maxlength="120" bind:value=name/>
                </label>
                <button type="submit" class="secondary" disabled=p.pending>{t("common.save")}</button>
            </form>
            <Show when=move || saved.get()><p class="notice success" role="status">{t("profile.saved")}</p></Show>
            {error_view(error)}
        </section>
        <section class="card" aria-labelledby="profile-address">
            <h2 id="profile-address">{t("profile.address.title")}</h2>
            <p class="hint">{t("profile.address_locked")}</p>
            <dl data-testid="profile-address">
                <dt>{t("auth.field.cep")}</dt><dd>{p.cep.clone()}</dd>
                <dt>{t("profile.address.street")}</dt><dd>{or_dash(p.street.clone())}</dd>
                <dt>{t("profile.address.neighborhood")}</dt><dd>{or_dash(p.neighborhood.clone())}</dd>
                <dt>{t("profile.address.city")}</dt><dd data-testid="profile-city">{format!("{} / {}", p.municipality, p.state)}</dd>
                <dt>{t("profile.address.country")}</dt><dd>{p.country.clone()}</dd>
            </dl>
        </section>
        <section class="card" aria-labelledby="profile-stats">
            <h2 id="profile-stats">{t("profile.stats")}</h2>
            <p class="notice empty">{t("profile.stats_empty")}</p>
        </section>
        <section class="card" aria-labelledby="profile-security">
            <h2 id="profile-security">{t("profile.security")}</h2>
            <p>{t("profile.visibility.profile")}": "{visibility_label(p.profile_visibility)}" · "
               {t("profile.visibility.vote")}": "{visibility_label(p.default_vote_visibility)}</p>
            <ul>
                <li><a href="/perfil/privacidade">{t("profile.privacy_link")}</a></li>
                <li><a href="/alterar-senha">{t("account.password.title")}</a></li>
                <li><a href="/perfil/codigos">{t("account.codes.title")}</a></li>
            </ul>
        </section>
    }
}

#[component]
pub fn PrivacyPage() -> impl IntoView {
    let profile = Resource::new(|| (), |_| my_profile());
    view! {
        <section class="page">
            <h1>{t("profile.privacy.title")}</h1>
            <Transition fallback=|| view! { <p>{t("common.loading")}</p> }>
                {move || profile.get().map(|p| match p {
                    Ok(p) => view! { <PrivacyForm p/> }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn parse_visibility(value: &str) -> Visibility {
    if value == "public" {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

#[component]
fn PrivacyForm(p: ProfileView) -> impl IntoView {
    let profile_vis = RwSignal::new(p.profile_visibility.as_db().to_owned());
    let vote_vis = RwSignal::new(p.default_vote_visibility.as_db().to_owned());
    let confirming = RwSignal::new(false);
    let saved = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);
    let on_granted = Callback::new(move |_| {
        let (a, b) = (
            parse_visibility(&profile_vis.get_untracked()),
            parse_visibility(&vote_vis.get_untracked()),
        );
        spawn_local(async move {
            match save_privacy(a, b).await {
                Ok(()) => {
                    confirming.set(false);
                    saved.set(true);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        saved.set(false);
        error.set(None);
        confirming.set(true);
    };
    let options = move || {
        view! {
            <option value="private">{t("profile.visibility.private")}</option>
            <option value="public">{t("profile.visibility.public")}</option>
        }
    };
    view! {
        <form on:submit=submit>
            <label>{t("profile.visibility.profile")}
                <select data-testid="privacy-profile" prop:value=move || profile_vis.get()
                    on:change=move |ev| profile_vis.set(event_target_value(&ev))>{options()}</select>
            </label>
            <label>{t("profile.visibility.vote")}
                <span class="hint">{t("profile.visibility.vote_hint")}</span>
                <select data-testid="privacy-vote" prop:value=move || vote_vis.get()
                    on:change=move |ev| vote_vis.set(event_target_value(&ev))>{options()}</select>
            </label>
            <button type="submit" data-testid="privacy-save" disabled=move || confirming.get()>{t("common.save")}</button>
        </form>
        <Show when=move || confirming.get()>
            <ReauthPanel action="profile:privacy".to_owned() resource=p.user_id.clone() on_granted/>
        </Show>
        <Show when=move || saved.get()><p class="notice success" role="status" data-testid="privacy-saved">{t("profile.saved")}</p></Show>
        {error_view(error)}
    }
}
