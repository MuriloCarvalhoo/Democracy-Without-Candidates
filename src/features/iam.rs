//! IAM administration (`/admin/iam/...`): customer policies with a JSON editor,
//! groups, user attachments and a policy simulator. Every change goes through one
//! server function after a face + TOTP reauthentication (`iam:manage`). The server
//! authorizes each call; hiding links is cosmetic.

use leptos::{ev::SubmitEvent, prelude::*};
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map, use_query_map},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::iam::{
        GroupSummary, GroupView, POLICY_VERSION, Permission, PolicySummary, PolicyView, Simulation,
        UserRef, UserView,
    },
    error::AppError,
    features::{
        admin::{AdminNav, Denied},
        reauth::ReauthPanel,
        ui::{Empty, Loading, action_error},
    },
    i18n::t,
};

/// One IAM change, applied by `iam_change` after reauthentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IamCommand {
    CreatePolicy {
        name: String,
        description: String,
        document: String,
    },
    UpdatePolicy {
        id: String,
        description: String,
        document: String,
    },
    DeletePolicy {
        id: String,
    },
    CreateGroup {
        name: String,
        description: String,
    },
    DeleteGroup {
        id: String,
    },
    GroupPolicy {
        group_id: String,
        policy_id: String,
        attach: bool,
    },
    GroupMember {
        group_id: String,
        email: String,
        member: bool,
    },
    UserPolicy {
        user_id: String,
        policy_id: String,
        attach: bool,
    },
}

#[cfg(feature = "ssr")]
mod srv {
    use crate::{error::AppError, features::face::bounded};

    pub fn id(value: &str) -> Result<&str, AppError> {
        if !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            Ok(value)
        } else {
            Err(AppError::NotFound)
        }
    }

    pub fn text(value: &str, field: &str) -> Result<String, AppError> {
        bounded(value, field, 1200).map(str::to_owned)
    }
}

#[server(input = leptos::server_fn::codec::Json, endpoint = "iam/change")]
pub async fn iam_change(cmd: IamCommand) -> Result<Option<String>, AppError> {
    use crate::{
        application::iam, domain::iam::DOCUMENT_MAX_BYTES, features::face::bounded, server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let (db, now) = (&state.db, state.now());
    match cmd {
        IamCommand::CreatePolicy {
            name,
            description,
            document,
        } => iam::create_policy(
            db,
            now,
            &actor,
            &srv::text(&name, "name")?,
            &srv::text(&description, "description")?,
            bounded(&document, "document", DOCUMENT_MAX_BYTES * 2)?,
        )
        .await
        .map(Some),
        IamCommand::UpdatePolicy {
            id,
            description,
            document,
        } => iam::update_policy(
            db,
            now,
            &actor,
            srv::id(&id)?,
            &srv::text(&description, "description")?,
            bounded(&document, "document", DOCUMENT_MAX_BYTES * 2)?,
        )
        .await
        .map(|_| None),
        IamCommand::DeletePolicy { id } => iam::delete_policy(db, now, &actor, srv::id(&id)?)
            .await
            .map(|_| None),
        IamCommand::CreateGroup { name, description } => iam::create_group(
            db,
            now,
            &actor,
            &srv::text(&name, "name")?,
            &srv::text(&description, "description")?,
        )
        .await
        .map(Some),
        IamCommand::DeleteGroup { id } => iam::delete_group(db, now, &actor, srv::id(&id)?)
            .await
            .map(|_| None),
        IamCommand::GroupPolicy {
            group_id,
            policy_id,
            attach,
        } => iam::set_group_policy(
            db,
            now,
            &actor,
            srv::id(&group_id)?,
            srv::id(&policy_id)?,
            attach,
        )
        .await
        .map(|_| None),
        IamCommand::GroupMember {
            group_id,
            email,
            member,
        } => iam::set_group_member(
            db,
            now,
            &actor,
            srv::id(&group_id)?,
            bounded(&email, "email", 254)?,
            member,
        )
        .await
        .map(|_| None),
        IamCommand::UserPolicy {
            user_id,
            policy_id,
            attach,
        } => iam::set_user_policy(
            db,
            now,
            &actor,
            srv::id(&user_id)?,
            srv::id(&policy_id)?,
            attach,
        )
        .await
        .map(|_| None),
    }
}

#[server(endpoint = "iam/policies")]
pub async fn iam_policies() -> Result<Vec<PolicySummary>, AppError> {
    use crate::{application::iam, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::list_policies(&state.db.read().await?, &actor).await
}

#[server(endpoint = "iam/policy")]
pub async fn iam_policy(id: String) -> Result<PolicyView, AppError> {
    use crate::{application::iam, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::get_policy(&state.db.read().await?, &actor, srv::id(&id)?).await
}

#[server(endpoint = "iam/groups")]
pub async fn iam_groups() -> Result<Vec<GroupSummary>, AppError> {
    use crate::{application::iam, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::list_groups(&state.db.read().await?, &actor).await
}

#[server(endpoint = "iam/group")]
pub async fn iam_group(id: String) -> Result<GroupView, AppError> {
    use crate::{application::iam, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::get_group(&state.db.read().await?, &actor, srv::id(&id)?).await
}

#[server(endpoint = "iam/users")]
pub async fn iam_users(query: String) -> Result<Vec<UserRef>, AppError> {
    use crate::{application::iam, features::face::bounded, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::search_users(
        &state.db.read().await?,
        &actor,
        bounded(&query, "query", 254)?,
    )
    .await
}

#[server(endpoint = "iam/user")]
pub async fn iam_user(id: String) -> Result<UserView, AppError> {
    use crate::{application::iam, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::user_detail(&state.db.read().await?, &actor, srv::id(&id)?).await
}

#[server(endpoint = "iam/simulate")]
pub async fn iam_simulate(
    email: String,
    action: String,
    resource: String,
) -> Result<Simulation, AppError> {
    use crate::{application::iam, features::face::bounded, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    iam::simulate(
        &state.db.read().await?,
        &actor,
        bounded(&email, "email", 254)?,
        bounded(&action, "action", 64)?,
        bounded(&resource, "resource", 64)?,
    )
    .await
}

const TEMPLATE: &str = r#"{
  "Version": "2026-10-25",
  "Statement": [
    {
      "Sid": "Exemplo",
      "Effect": "Allow",
      "Action": ["moderation:act"],
      "Resource": "*"
    }
  ]
}"#;

#[component]
fn IamNav() -> impl IntoView {
    view! {
        <nav class="row" aria-label=t("iam.nav")>
            <A href="/admin/iam/politicas">{t("iam.nav.policies")}</A>
            <A href="/admin/iam/grupos">{t("iam.nav.groups")}</A>
            <A href="/admin/iam/usuarios">{t("iam.nav.users")}</A>
            <A href="/admin/iam/simulador">{t("iam.nav.simulator")}</A>
        </nav>
    }
}

/// Pending change plus the action that applies it after reauthentication.
#[derive(Clone, Copy)]
struct Changes {
    pending: RwSignal<Option<IamCommand>>,
    apply: ServerAction<IamChange>,
}

impl Changes {
    fn new() -> Self {
        Self {
            pending: RwSignal::new(None),
            apply: ServerAction::<IamChange>::new(),
        }
    }

    fn request(&self, cmd: IamCommand) {
        self.pending.set(Some(cmd));
    }

    fn version(&self) -> usize {
        self.apply.version().get()
    }
}

/// Reauthentication panel for the pending change, and the outcome of the last one.
#[component]
fn ChangePanel(changes: Changes) -> impl IntoView {
    let on_granted = Callback::new(move |_| {
        if let Some(cmd) = changes.pending.get_untracked() {
            changes.pending.set(None);
            changes.apply.dispatch(IamChange { cmd });
        }
    });
    view! {
        {move || changes.pending.get().map(|_| view! {
            <p class="hint">{t("iam.reauth_hint")}</p>
            <ReauthPanel action="iam:manage".to_owned() resource="iam".to_owned() on_granted/>
            <button type="button" class="secondary" on:click=move |_| changes.pending.set(None)>{t("iam.cancel")}</button>
        })}
        {move || action_error(changes.apply.value().get())}
        {move || matches!(changes.apply.value().get(), Some(Ok(_))).then(|| view! {
            <p class="notice success" role="status" data-testid="iam-done">{t("iam.done")}</p>
        })}
    }
}

#[component]
fn IamFrame(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="page">
            <h1>{title}</h1>
            <AdminNav/>
            <IamNav/>
            {children()}
        </section>
    }
}

fn managed_badge(managed: bool) -> Option<impl IntoView> {
    managed.then(|| view! { <span class="badge">{t("iam.policy.managed_badge")}</span> })
}

#[component]
pub fn IamPoliciesPage() -> impl IntoView {
    let changes = Changes::new();
    let list = Resource::new(move || changes.version(), |_| iam_policies());
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let document = RwSignal::new(TEMPLATE.to_owned());
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        changes.request(IamCommand::CreatePolicy {
            name: name.get_untracked(),
            description: description.get_untracked(),
            document: document.get_untracked(),
        });
    };
    view! {
        <IamFrame title=t("iam.policies.title")>
            <Transition fallback=|| view! { <Loading/> }>
                {move || list.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(policies) => view! { <PolicyTable policies/> }.into_any(),
                })}
            </Transition>
            <section class="card" aria-labelledby="new-policy">
                <h2 id="new-policy">{t("iam.policies.create")}</h2>
                <p class="hint">{t("iam.policies.hint")}" "{POLICY_VERSION}</p>
                <form on:submit=submit>
                    <label>{t("iam.field.name")}
                        <input type="text" required maxlength="64" data-testid="iam-policy-name" bind:value=name/>
                    </label>
                    <label>{t("iam.field.description")}
                        <input type="text" maxlength="300" data-testid="iam-policy-description" bind:value=description/>
                    </label>
                    <label>{t("iam.field.document")}
                        <textarea rows="14" spellcheck="false" class="code" data-testid="iam-policy-document" bind:value=document></textarea>
                    </label>
                    <button type="submit" data-testid="iam-policy-create">{t("iam.policies.create")}</button>
                </form>
                <ChangePanel changes/>
            </section>
            <ActionCatalog/>
        </IamFrame>
    }
}

#[component]
fn PolicyTable(policies: Vec<PolicySummary>) -> impl IntoView {
    view! {
        <div class="table-scroll">
            <table data-testid="iam-policies">
                <thead><tr>
                    <th scope="col">{t("iam.field.name")}</th>
                    <th scope="col">{t("iam.field.description")}</th>
                    <th scope="col">{t("iam.policies.attachments")}</th>
                    <th scope="col">{t("iam.policies.version")}</th>
                </tr></thead>
                <tbody>
                    {policies.into_iter().map(|p| {
                        let testid = format!("iam-policy-{}", p.name);
                        let badge = managed_badge(p.managed);
                        view! {
                            <tr data-testid=testid>
                                <td><A href=format!("/admin/iam/politicas/{}", p.id)>{p.name}</A>" "{badge}</td>
                                <td>{p.description}</td>
                                <td>{p.attachments}</td>
                                <td>{p.version}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// Reference of the fixed action catalog accepted in documents.
#[component]
fn ActionCatalog() -> impl IntoView {
    view! {
        <details class="card">
            <summary>{t("iam.catalog.title")}</summary>
            <dl class="facts">
                {Permission::ALL.into_iter().map(|p| view! {
                    <dt><code>{p.as_str()}</code></dt><dd>{t(p.label_key())}</dd>
                }).collect_view()}
            </dl>
            <p class="hint">{t("iam.catalog.hint")}</p>
        </details>
    }
}

fn route_id() -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("id").unwrap_or_default())
}

#[component]
pub fn IamPolicyPage() -> impl IntoView {
    let id = route_id();
    let changes = Changes::new();
    let policy = Resource::new(
        move || (id.get(), changes.version()),
        |(id, _)| iam_policy(id),
    );
    // After a successful delete, go back to the list.
    let navigate = use_navigate();
    Effect::new(move |_| {
        if matches!(changes.apply.value().get(), Some(Ok(_)))
            && matches!(policy.get(), Some(Err(AppError::NotFound)))
        {
            navigate("/admin/iam/politicas", Default::default());
        }
    });
    view! {
        <IamFrame title=t("iam.policy.title")>
            <Transition fallback=|| view! { <Loading/> }>
                {move || policy.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(p) => view! { <PolicyDetail p changes/> }.into_any(),
                })}
            </Transition>
            <ChangePanel changes/>
        </IamFrame>
    }
}

#[component]
fn PolicyDetail(p: PolicyView, changes: Changes) -> impl IntoView {
    let description = RwSignal::new(p.description.clone());
    let document = RwSignal::new(p.document.clone());
    let id = StoredValue::new(p.id.clone());
    let save = move |ev: SubmitEvent| {
        ev.prevent_default();
        changes.request(IamCommand::UpdatePolicy {
            id: id.get_value(),
            description: description.get_untracked(),
            document: document.get_untracked(),
        });
    };
    let delete = move |_| changes.request(IamCommand::DeletePolicy { id: id.get_value() });
    view! {
        <section class="card" data-testid="iam-policy-detail">
            <h2>{p.name.clone()}" "{managed_badge(p.managed)}</h2>
            <p class="hint">{t("iam.policies.version")}" "{p.version}" · "{p.updated_at.clone()}</p>
            {if p.managed {
                view! {
                    <p>{p.description.clone()}</p>
                    <p class="notice">{t("iam.policy.managed_hint")}</p>
                    <pre class="code" data-testid="iam-policy-json">{p.document.clone()}</pre>
                }.into_any()
            } else {
                view! {
                    <form on:submit=save>
                        <label>{t("iam.field.description")}
                            <input type="text" maxlength="300" bind:value=description/>
                        </label>
                        <label>{t("iam.field.document")}
                            <textarea rows="14" spellcheck="false" class="code" data-testid="iam-policy-document" bind:value=document></textarea>
                        </label>
                        <div class="row">
                            <button type="submit" data-testid="iam-policy-save">{t("iam.save")}</button>
                            <button type="button" class="danger" data-testid="iam-policy-delete" on:click=delete>{t("iam.delete")}</button>
                        </div>
                    </form>
                }.into_any()
            }}
        </section>
    }
}

#[component]
pub fn IamGroupsPage() -> impl IntoView {
    let changes = Changes::new();
    let list = Resource::new(move || changes.version(), |_| iam_groups());
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        changes.request(IamCommand::CreateGroup {
            name: name.get_untracked(),
            description: description.get_untracked(),
        });
    };
    view! {
        <IamFrame title=t("iam.groups.title")>
            <Transition fallback=|| view! { <Loading/> }>
                {move || list.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(groups) if groups.is_empty() => view! { <Empty message=t("iam.groups.empty")/> }.into_any(),
                    Ok(groups) => view! {
                        <ul class="list" data-testid="iam-groups">
                            {groups.into_iter().map(|g| view! {
                                <li class="card">
                                    <A href=format!("/admin/iam/grupos/{}", g.id)>{g.name}</A>
                                    <span class="hint">" — "{g.description}" · "{t("iam.groups.members")}": "{g.members}
                                        " · "{t("iam.nav.policies")}": "{g.policies}</span>
                                </li>
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                })}
            </Transition>
            <section class="card" aria-labelledby="new-group">
                <h2 id="new-group">{t("iam.groups.create")}</h2>
                <form on:submit=submit>
                    <label>{t("iam.field.name")}
                        <input type="text" required maxlength="64" data-testid="iam-group-name" bind:value=name/>
                    </label>
                    <label>{t("iam.field.description")}
                        <input type="text" maxlength="300" bind:value=description/>
                    </label>
                    <button type="submit" data-testid="iam-group-create">{t("iam.groups.create")}</button>
                </form>
                <ChangePanel changes/>
            </section>
        </IamFrame>
    }
}

/// `<select>` of every policy (for attaching), submitting the chosen id.
#[component]
fn AttachPolicy(policies: Vec<PolicySummary>, on_attach: Callback<String>) -> impl IntoView {
    let chosen = RwSignal::new(policies.first().map(|p| p.id.clone()).unwrap_or_default());
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let id = chosen.get_untracked();
        if !id.is_empty() {
            on_attach.run(id);
        }
    };
    view! {
        <form class="row" on:submit=submit>
            <label>{t("iam.attach.policy")}
                <select data-testid="iam-attach-policy" bind:value=chosen>
                    {policies.into_iter().map(|p| view! { <option value=p.id>{p.name}</option> }).collect_view()}
                </select>
            </label>
            <button type="submit" data-testid="iam-attach-submit">{t("iam.attach")}</button>
        </form>
    }
}

#[component]
pub fn IamGroupPage() -> impl IntoView {
    let id = route_id();
    let changes = Changes::new();
    let group = Resource::new(
        move || (id.get(), changes.version()),
        |(id, _)| iam_group(id),
    );
    let policies = Resource::new(|| (), |_| iam_policies());
    let email = RwSignal::new(String::new());
    let navigate = use_navigate();
    Effect::new(move |_| {
        if matches!(changes.apply.value().get(), Some(Ok(_)))
            && matches!(group.get(), Some(Err(AppError::NotFound)))
        {
            navigate("/admin/iam/grupos", Default::default());
        }
    });
    let add_member = move |ev: SubmitEvent| {
        ev.prevent_default();
        changes.request(IamCommand::GroupMember {
            group_id: id.get_untracked(),
            email: email.get_untracked(),
            member: true,
        });
    };
    let on_attach = Callback::new(move |policy_id| {
        changes.request(IamCommand::GroupPolicy {
            group_id: id.get_untracked(),
            policy_id,
            attach: true,
        })
    });
    view! {
        <IamFrame title=t("iam.group.title")>
            <Transition fallback=|| view! { <Loading/> }>
                {move || group.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(g) => {
                        let group_id = g.id.clone();
                        view! {
                            <section class="card" data-testid="iam-group-detail">
                                <h2>{g.name}</h2>
                                <p>{g.description}</p>
                                <h3>{t("iam.nav.policies")}</h3>
                                <ul class="list">
                                    {g.policies.into_iter().map(|p| {
                                        let (gid, pid) = (group_id.clone(), p.id.clone());
                                        view! {
                                            <li class="row" data-testid=format!("iam-group-policy-{}", p.name)>
                                                <span>{p.name.clone()}</span>
                                                <button type="button" class="danger" data-testid="iam-detach"
                                                    on:click=move |_| changes.request(IamCommand::GroupPolicy { group_id: gid.clone(), policy_id: pid.clone(), attach: false })>
                                                    {t("iam.detach")}</button>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                                <h3>{t("iam.groups.members")}</h3>
                                <ul class="list">
                                    {g.members.into_iter().map(|u| {
                                        let (gid, mail) = (group_id.clone(), u.email.clone());
                                        view! {
                                            <li class="row">
                                                <A href=format!("/admin/iam/usuarios/{}", u.id)>{u.email}</A>
                                                <button type="button" class="danger"
                                                    on:click=move |_| changes.request(IamCommand::GroupMember { group_id: gid.clone(), email: mail.clone(), member: false })>
                                                    {t("iam.remove")}</button>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                                <button type="button" class="danger" data-testid="iam-group-delete"
                                    on:click=move |_| changes.request(IamCommand::DeleteGroup { id: group_id.clone() })>
                                    {t("iam.delete")}</button>
                            </section>
                        }.into_any()
                    }
                })}
            </Transition>
            <section class="card">
                <Transition fallback=|| ()>
                    {move || policies.get().and_then(|r| r.ok()).map(|policies| view! { <AttachPolicy policies on_attach/> })}
                </Transition>
                <form class="row" on:submit=add_member>
                    <label>{t("iam.groups.add_member")}
                        <input type="email" required autocomplete="off" data-testid="iam-member-email" bind:value=email/>
                    </label>
                    <button type="submit" data-testid="iam-member-add">{t("iam.add")}</button>
                </form>
                <ChangePanel changes/>
            </section>
        </IamFrame>
    }
}

#[component]
pub fn IamUsersPage() -> impl IntoView {
    let query_map = use_query_map();
    let query = RwSignal::new(String::new());
    let searched = Memo::new(move |_| query_map.read().get("q").unwrap_or_default());
    let results = Resource::new(
        move || searched.get(),
        |q| async move {
            if q.is_empty() {
                Ok(Vec::new())
            } else {
                iam_users(q).await
            }
        },
    );
    let navigate = use_navigate();
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let q = query.get_untracked();
        navigate(
            &format!("/admin/iam/usuarios?q={}", urlencode(&q)),
            Default::default(),
        );
    };
    view! {
        <IamFrame title=t("iam.users.title")>
            <form class="row" role="search" on:submit=submit>
                <label>{t("iam.users.query")}
                    <input type="search" minlength="2" required data-testid="iam-user-query" bind:value=query/>
                </label>
                <button type="submit" data-testid="iam-user-search">{t("iam.users.search")}</button>
            </form>
            <Transition fallback=|| view! { <Loading/> }>
                {move || results.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(users) if users.is_empty() => view! { <Empty/> }.into_any(),
                    Ok(users) => view! {
                        <ul class="list" data-testid="iam-users">
                            {users.into_iter().map(|u| view! {
                                <li class="card">
                                    <A href=format!("/admin/iam/usuarios/{}", u.id) attr:data-testid="iam-user-link">{u.email}</A>
                                    <span class="hint">" — "{u.name}" ("{u.status}")"</span>
                                </li>
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                })}
            </Transition>
        </IamFrame>
    }
}

/// Minimal percent-encoding for the search query string.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b))
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[component]
pub fn IamUserPage() -> impl IntoView {
    let id = route_id();
    let changes = Changes::new();
    let user = Resource::new(
        move || (id.get(), changes.version()),
        |(id, _)| iam_user(id),
    );
    let policies = Resource::new(|| (), |_| iam_policies());
    let on_attach = Callback::new(move |policy_id| {
        changes.request(IamCommand::UserPolicy {
            user_id: id.get_untracked(),
            policy_id,
            attach: true,
        })
    });
    view! {
        <IamFrame title=t("iam.user.title")>
            <Transition fallback=|| view! { <Loading/> }>
                {move || user.get().map(|r| match r {
                    Err(error) => view! { <Denied error/> }.into_any(),
                    Ok(v) => view! { <UserDetail v changes/> }.into_any(),
                })}
            </Transition>
            <section class="card">
                <Transition fallback=|| ()>
                    {move || policies.get().and_then(|r| r.ok()).map(|policies| view! { <AttachPolicy policies on_attach/> })}
                </Transition>
                <ChangePanel changes/>
            </section>
        </IamFrame>
    }
}

#[component]
fn UserDetail(v: UserView, changes: Changes) -> impl IntoView {
    let user_id = v.user.id.clone();
    view! {
        <section class="card" data-testid="iam-user-detail">
            <h2>{v.user.name}</h2>
            <p>{v.user.email}" · "{v.user.status}</p>
            <h3>{t("iam.nav.policies")}</h3>
            <ul class="list">
                {v.policies.into_iter().map(|p| {
                    let (uid, pid) = (user_id.clone(), p.id.clone());
                    let testid = format!("iam-user-policy-{}", p.name);
                    view! {
                        <li class="row" data-testid=testid>
                            <A href=format!("/admin/iam/politicas/{}", p.id)>{p.name}</A>
                            <button type="button" class="danger" data-testid="iam-detach"
                                on:click=move |_| changes.request(IamCommand::UserPolicy { user_id: uid.clone(), policy_id: pid.clone(), attach: false })>
                                {t("iam.detach")}</button>
                        </li>
                    }
                }).collect_view()}
            </ul>
            <h3>{t("iam.nav.groups")}</h3>
            {if v.groups.is_empty() {
                view! { <Empty message=t("iam.user.no_groups")/> }.into_any()
            } else {
                view! {
                    <ul class="list">
                        {v.groups.into_iter().map(|g| view! {
                            <li><A href=format!("/admin/iam/grupos/{}", g.id)>{g.name}</A></li>
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
            <h3>{t("iam.user.effective")}</h3>
            <p class="hint">{t("iam.user.effective_hint")}</p>
            <ul class="list" data-testid="iam-effective">
                {v.allowed.into_iter().map(|a| view! { <li><code>{a}</code></li> }).collect_view()}
            </ul>
        </section>
    }
}

#[component]
pub fn IamSimulatorPage() -> impl IntoView {
    let simulate = ServerAction::<IamSimulate>::new();
    view! {
        <IamFrame title=t("iam.sim.title")>
            <section class="card">
                <p class="hint">{t("iam.sim.hint")}</p>
                <ActionForm action=simulate>
                    <label>{t("iam.sim.email")}
                        <input type="email" name="email" required autocomplete="off" data-testid="iam-sim-email"/>
                    </label>
                    <label>{t("iam.sim.action")}
                        <select name="action" data-testid="iam-sim-action">
                            {Permission::ALL.into_iter().map(|p| view! { <option value=p.as_str()>{p.as_str()}</option> }).collect_view()}
                        </select>
                    </label>
                    <label>{t("iam.sim.resource")}
                        <input type="text" name="resource" placeholder="*" maxlength="64" data-testid="iam-sim-resource"/>
                    </label>
                    <button type="submit" data-testid="iam-sim-submit" disabled=move || simulate.pending().get()>{t("iam.sim.submit")}</button>
                </ActionForm>
                {move || action_error(simulate.value().get())}
                {move || simulate.value().get().and_then(|r| r.ok()).map(|s| view! { <SimulationResult s/> })}
            </section>
        </IamFrame>
    }
}

#[component]
fn SimulationResult(s: Simulation) -> impl IntoView {
    let verdict = if s.decision.allowed {
        t("iam.sim.allowed")
    } else {
        t("iam.sim.denied")
    };
    let reason = match (s.decision.effect, s.decision.policy) {
        (None, _) | (_, None) => t("iam.sim.implicit").to_owned(),
        (Some(_), Some(policy)) => format!(
            "{} {policy}{}",
            t("iam.sim.statement"),
            s.decision
                .sid
                .map(|sid| format!(" / {sid}"))
                .unwrap_or_default()
        ),
    };
    view! {
        <dl class="facts" data-testid="iam-sim-result">
            <dt>{t("iam.sim.email")}</dt><dd>{s.user.email}" ("{s.user.status}")"</dd>
            <dt>{t("iam.sim.request")}</dt><dd><code>{s.action}</code>" @ "<code>{s.resource}</code></dd>
            <dt>{t("iam.sim.decision")}</dt><dd data-testid="iam-sim-verdict">{verdict}</dd>
            <dt>{t("iam.sim.reason")}</dt><dd data-testid="iam-sim-reason">{reason}</dd>
        </dl>
    }
}
