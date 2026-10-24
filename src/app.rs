use leptos::prelude::*;
use leptos_meta::{Link, Meta, MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    features::{
        account::{ChangePasswordPage, RecoveryCodesPage},
        audit::{AuditAdvancedPage, AuditPublicPage},
        enroll::EnrollPage,
        home::HomePage,
        login::{LoginPage, ResetPasswordPage},
        notifications::{NotificationPreferencesPage, NotificationsPage},
        profile::{PrivacyPage, ProfilePage},
        register::RegisterPage,
        shell::Layout,
    },
    i18n::t,
};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="pt-BR">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/dwc.css"/>
        <Link rel="manifest" href="/manifest.webmanifest"/>
        <Meta name="theme-color" content="#1f4e79"/>
        <Title text=t("app.name")/>
        <Router>
            <Layout>
                <Routes fallback=NotFound>
                    <Route path=path!("") view=HomePage/>
                    <Route path=path!("/auditoria") view=AuditPublicPage/>
                    <Route path=path!("/auditoria/avancada") view=AuditAdvancedPage/>
                    <Route path=path!("/notificacoes") view=NotificationsPage/>
                    <Route path=path!("/perfil/notificacoes") view=NotificationPreferencesPage/>
                    <Route path=path!("/login") view=LoginPage/>
                    <Route path=path!("/recuperar-senha") view=ResetPasswordPage/>
                    <Route path=path!("/cadastro") view=RegisterPage/>
                    <Route path=path!("/cadastro/biometria") view=EnrollPage/>
                    <Route path=path!("/perfil") view=ProfilePage/>
                    <Route path=path!("/perfil/privacidade") view=PrivacyPage/>
                    <Route path=path!("/perfil/codigos") view=RecoveryCodesPage/>
                    <Route path=path!("/alterar-senha") view=ChangePasswordPage/>
                </Routes>
            </Layout>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
            response.set_status(axum::http::StatusCode::NOT_FOUND);
        }
    }
    view! {
        <section class="page">
            <h1>{t("notfound.title")}</h1>
        </section>
    }
}
