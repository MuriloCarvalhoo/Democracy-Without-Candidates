#![recursion_limit = "256"]

pub mod app;
pub mod client_errors;
pub mod domain;
pub mod error;
pub mod features;
pub mod i18n;

#[cfg(feature = "ssr")]
pub mod application;
#[cfg(feature = "ssr")]
pub mod infrastructure;
#[cfg(feature = "ssr")]
pub mod server;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    client_errors::install();
    leptos::mount::hydrate_body(app::App);
}
