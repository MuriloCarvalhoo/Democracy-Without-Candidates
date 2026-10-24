//! PWA installation support (online-only). Registers `/sw.js` from the hydrated
//! app in production, or in other environments when explicitly enabled with
//! `localStorage.setItem("dwc.sw", "1")`. The worker never caches pages or server
//! function responses and never queues actions.

use leptos::prelude::*;

use crate::features::shell::InstanceResource;

#[component]
pub fn PwaRegistration() -> impl IntoView {
    if let Some(info) = use_context::<InstanceResource>() {
        // Effects run only in the browser, after hydration.
        Effect::new(move |done: Option<bool>| {
            if done == Some(true) {
                return true;
            }
            match info.get() {
                Some(Ok(instance)) => {
                    if !instance.synthetic || explicitly_enabled() {
                        register();
                    }
                    true
                }
                _ => false,
            }
        });
    }
}

#[cfg(feature = "hydrate")]
fn explicitly_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("dwc.sw").ok().flatten())
        .as_deref()
        == Some("1")
}

#[cfg(feature = "hydrate")]
fn register() {
    use web_sys::js_sys::Reflect;
    let Some(window) = web_sys::window() else {
        return;
    };
    let navigator = window.navigator();
    // Absent outside secure contexts (plain HTTP on a non-loopback host).
    if !Reflect::has(&navigator, &"serviceWorker".into()).unwrap_or(false) {
        return;
    }
    let _ = navigator.service_worker().register("/sw.js");
}

#[cfg(not(feature = "hydrate"))]
fn explicitly_enabled() -> bool {
    false
}

#[cfg(not(feature = "hydrate"))]
fn register() {}
