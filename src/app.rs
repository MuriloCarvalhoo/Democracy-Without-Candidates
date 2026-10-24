use leptos::prelude::*;
use leptos_meta::{HashedStylesheet, Link, Meta, MetaTags, Title, provide_meta_context};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::{
    features::{
        admin::{AdminPage, AuditorsPage},
        home::HomePage,
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
        <Meta name="theme-color" content="#1f4e79"/>
        <Title text=t("app.name")/>
        <Router>
            <Layout>
                <Routes fallback=NotFound>
                    <Route path=path!("") view=HomePage/>
                    <Route path=path!("admin") view=AdminPage/>
                    <Route path=path!("admin/auditores") view=AuditorsPage/>
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
