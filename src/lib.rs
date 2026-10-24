#![recursion_limit = "256"]

pub mod app;
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
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
