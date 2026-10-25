use leptos::prelude::*;
use leptos_meta::{HashedStylesheet, Link, Meta, MetaTags, Title, provide_meta_context};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    features::{
        account::{ChangePasswordPage, RecoveryCodesPage},
        admin::{AdminPage, AuditorsPage},
        audit::{AuditAdvancedPage, AuditPublicPage},
        debate::{AmendmentsPage, DebatePage, ModerationPage},
        enroll::EnrollPage,
        home::HomePage,
        login::{LoginPage, ResetPasswordPage},
        metalaws::{MetaLawPage, MetaLawsPage, NewMetaLawPage},
        notifications::{NotificationPreferencesPage, NotificationsPage},
        profile::{PrivacyPage, ProfilePage},
        proposals::{
            DashboardPage, EditProposalPage, NewProposalPage, ProposalPage, ProposalsPage,
            SetupHistoryPage, SignersPage,
        },
        register::RegisterPage,
        shell::Layout,
        voting::{ResultPage, SetupPage, StatisticsPage, VotePage},
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
                // Content-hashed CSS/JS/WASM names (cargo-leptos `hash-files`) let the
                // service worker cache them as immutable.
                <HashedStylesheet id="leptos" options=options.clone()/>
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
        <Link rel="manifest" href="/manifest.webmanifest"/>
        <Meta name="theme-color" content="#2238b0"/>
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
                    <Route path=path!("/propostas") view=ProposalsPage/>
                    <Route path=path!("/propostas/nova") view=NewProposalPage/>
                    <Route path=path!("/propostas/:id") view=ProposalPage/>
                    <Route path=path!("/propostas/:id/editar") view=EditProposalPage/>
                    <Route path=path!("/propostas/:id/assinantes") view=SignersPage/>
                    <Route path=path!("/propostas/:id/debate") view=DebatePage/>
                    <Route path=path!("/propostas/:id/debate/emenda") view=AmendmentsPage/>
                    <Route path=path!("/propostas/:id/votar") view=VotePage/>
                    <Route path=path!("/propostas/:id/resultado") view=ResultPage/>
                    <Route path=path!("/metaleis") view=MetaLawsPage/>
                    <Route path=path!("/metaleis/nova") view=NewMetaLawPage/>
                    <Route path=path!("/metaleis/:id") view=MetaLawPage/>
                    <Route path=path!("/setup") view=SetupPage/>
                    <Route path=path!("/estatisticas") view=StatisticsPage/>
                    <Route path=path!("/dashboard") view=DashboardPage/>
                    <Route path=path!("/setup/historico") view=SetupHistoryPage/>
                    <Route path=path!("/admin") view=AdminPage/>
                    <Route path=path!("/admin/auditores") view=AuditorsPage/>
                    <Route path=path!("/admin/moderacao") view=ModerationPage/>
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
