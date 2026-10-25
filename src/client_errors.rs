//! Browser-side failures: a panic stops the WebAssembly app, so the hook shows
//! a visible notice (native links and reload keep working) and, on
//! development/test instances (`<meta name="dwc-client-log">`), reports panics,
//! uncaught errors and server errors to the server terminal.

use crate::error::AppError;

/// Reports server errors shown to the user (`Internal`, `Transport`, `Unavailable`).
pub fn report_app_error(error: &AppError) {
    #[cfg(feature = "hydrate")]
    if matches!(
        error,
        AppError::Internal | AppError::Transport(_) | AppError::Unavailable(_)
    ) {
        browser::report("app-error", &format!("{error:?}"));
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = error;
}

/// Installs the panic hook and window error listeners (call once, before hydration).
#[cfg(feature = "hydrate")]
pub fn install() {
    browser::install();
}

#[cfg(feature = "hydrate")]
mod browser {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use wasm_bindgen::{JsCast, closure::Closure};

    use crate::i18n::t;

    /// Set by the panic hook: the instance is unusable, so listeners stay quiet.
    static PANICKED: AtomicBool = AtomicBool::new(false);
    static SENT: AtomicU32 = AtomicU32::new(0);
    /// Reports per page load, so a failing loop cannot flood the terminal.
    const MAX_REPORTS: u32 = 20;
    /// `window` flag set by the panic hook, read by the native fallbacks below.
    const CRASHED: &str = "__dwcCrashed";
    const NATIVE_LINKS: &str = "if (!window.__dwcCrashed || e.defaultPrevented || e.button !== 0 \
        || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return; \
        const a = e.target.closest && e.target.closest('a[href]'); \
        if (!a || (a.target && a.target !== '_self') || a.hasAttribute('download')) return; \
        e.stopImmediatePropagation(); e.preventDefault(); location.assign(a.href);";
    const NATIVE_FORMS: &str = "if (window.__dwcCrashed) e.stopImmediatePropagation();";

    pub fn install() {
        std::panic::set_hook(Box::new(|info| {
            console_error_panic_hook::hook(info);
            if PANICKED.swap(true, Ordering::SeqCst) {
                return;
            }
            if let Some(window) = web_sys::window() {
                let _ = js_sys::Reflect::set(&window, &CRASHED.into(), &true.into());
            }
            send("panic", &info.to_string());
            show_crash_notice();
        }));
        let Some(window) = web_sys::window() else {
            return;
        };
        // Plain JavaScript (runs even when the WebAssembly state is broken):
        // after a panic, links and forms bypass the dead router natively.
        for (event, body) in [("click", NATIVE_LINKS), ("submit", NATIVE_FORMS)] {
            let f = js_sys::Function::new_with_args("e", body);
            let _ = window.add_event_listener_with_callback_and_bool(event, &f, true);
        }
        let on_error = Closure::<dyn Fn(web_sys::ErrorEvent)>::new(|e: web_sys::ErrorEvent| {
            report(
                "error",
                &format!(
                    "{} ({}:{}:{})",
                    e.message(),
                    e.filename(),
                    e.lineno(),
                    e.colno()
                ),
            );
        });
        let on_rejection = Closure::<dyn Fn(web_sys::PromiseRejectionEvent)>::new(
            |e: web_sys::PromiseRejectionEvent| {
                report("rejection", &format!("{:?}", e.reason()));
            },
        );
        let _ = window.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref());
        let _ = window.add_event_listener_with_callback(
            "unhandledrejection",
            on_rejection.as_ref().unchecked_ref(),
        );
        on_error.forget();
        on_rejection.forget();
    }

    pub fn report(kind: &str, message: &str) {
        if !PANICKED.load(Ordering::SeqCst) {
            send(kind, message);
        }
    }

    /// `sendBeacon` to the advertised endpoint; silently skipped elsewhere.
    fn send(kind: &str, message: &str) {
        if SENT.fetch_add(1, Ordering::SeqCst) >= MAX_REPORTS {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(endpoint) = window
            .document()
            .and_then(|d| d.query_selector("meta[name=dwc-client-log]").ok().flatten())
            .and_then(|m| m.get_attribute("content"))
        else {
            return;
        };
        let url = window.location().pathname().unwrap_or_default();
        let body = serde_json::json!({ "kind": kind, "message": message, "url": url }).to_string();
        let _ = window
            .navigator()
            .send_beacon_with_opt_str(&endpoint, Some(&body));
    }

    /// Plain DOM (the reactive system is gone): message and a reload link.
    fn show_crash_notice() {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let (Some(body), Ok(notice), Ok(link)) = (
            document.body(),
            document.create_element("p"),
            document.create_element("a"),
        ) else {
            return;
        };
        notice.set_class_name("banner crash");
        let _ = notice.set_attribute("role", "alert");
        let _ = notice.set_attribute("data-testid", "app-crashed");
        notice.set_text_content(Some(&format!("{} ", t("error.crashed"))));
        let _ = link.set_attribute("href", "");
        link.set_text_content(Some(t("error.reload")));
        let _ = notice.append_child(&link);
        let _ = body.insert_before(&notice, body.first_child().as_ref());
    }
}

/// Debug builds only: lets browser tests exercise the panic path.
#[cfg(all(feature = "hydrate", debug_assertions))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn dwc_debug_panic() {
    panic!("dwc_debug_panic: deliberate test panic");
}
