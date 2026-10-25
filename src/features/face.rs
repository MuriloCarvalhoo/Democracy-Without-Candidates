//! Face capture shared by enrollment, login, recovery, password change and
//! reauthentication. Each capture is bound to a single-use server challenge.
//! Synthetic provider (development/test only): deterministic bytes from a phrase.
//! Real provider: camera via getUserMedia → canvas JPEG → base64.

use leptos::{prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};

use crate::{error::AppError, features::ui::ErrorNotice, i18n::t};

/// True when the instance uses the synthetic face provider (never in production).
#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn face_mode() -> Result<bool, AppError> {
    Ok(crate::server::context::state()?.face.is_synthetic())
}

/// New single-use challenge. `flow` is `session` (signed-in account) or `auth`
/// (login/password reset in progress, bound to the auth cookie).
#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(flow = %flow), err(level = "debug")))]
pub async fn face_challenge(flow: String) -> Result<String, AppError> {
    use crate::{
        application::identity::{auth::auth_subject, issue_face_challenge, user_subject},
        server::context,
    };
    let state = context::state()?;
    let subject = match flow.as_str() {
        "session" => user_subject(&context::require_actor().await?.user_id),
        "auth" => auth_subject(
            &context::auth_token()
                .await?
                .ok_or_else(|| AppError::validation("flow", t("auth.error.flow_expired")))?,
        ),
        _ => return Err(AppError::validation("flow", "fluxo inválido")),
    };
    issue_face_challenge(&state.db, state.now(), &subject).await
}

/// Decodes a capture sent by the browser, bounded in size.
#[cfg(feature = "ssr")]
pub fn decode_image(image_b64: &str) -> Result<Vec<u8>, AppError> {
    use crate::application::identity::FACE_IMAGE_MAX;
    use base64::Engine;
    let invalid = || AppError::validation("face", t("auth.error.face_image"));
    if image_b64.is_empty() || image_b64.len() > FACE_IMAGE_MAX * 4 / 3 + 4 {
        return Err(invalid());
    }
    base64::engine::general_purpose::STANDARD
        .decode(image_b64.trim())
        .map_err(|_| invalid())
}

/// Bounded text input from a server function argument.
#[cfg(feature = "ssr")]
pub fn bounded<'a>(value: &'a str, field: &str, max: usize) -> Result<&'a str, AppError> {
    if value.len() > max {
        Err(AppError::validation(field, t("common.too_long")))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceCaptured {
    pub challenge: String,
    pub image_b64: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
enum CameraState {
    Idle,
    Starting,
    Live,
    Denied,
    Unsupported,
    Timeout,
    Failed,
}

#[component]
pub fn FaceCapture(
    /// `session` or `auth`.
    flow: &'static str,
    on_capture: Callback<FaceCaptured>,
    #[prop(into, optional)] disabled: Signal<bool>,
) -> impl IntoView {
    let mode = Resource::new(|| (), |_| face_mode());
    let error = RwSignal::new(None::<AppError>);
    let busy = RwSignal::new(false);

    let deliver = move |image_b64: String| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match face_challenge(flow.to_owned()).await {
                Ok(challenge) => on_capture.run(FaceCaptured {
                    challenge,
                    image_b64,
                }),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="face-capture">
            <Suspense fallback=|| view! { <p>{t("common.loading")}</p> }>
                {move || mode.get().map(|m| match m {
                    Ok(true) => view! { <SyntheticFace deliver disabled=Signal::derive(move || disabled.get() || busy.get())/> }.into_any(),
                    Ok(false) => view! { <CameraFace deliver disabled=Signal::derive(move || disabled.get() || busy.get())/> }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Suspense>
            {move || error.get().map(|error| view! { <ErrorNotice error/> })}
        </div>
    }
}

#[component]
fn SyntheticFace(
    deliver: impl Fn(String) + Copy + Send + Sync + 'static,
    disabled: Signal<bool>,
) -> impl IntoView {
    let phrase = RwSignal::new(String::new());
    let use_face = move |_| {
        use base64::Engine;
        let value = phrase.get_untracked();
        if !value.trim().is_empty() {
            deliver(base64::engine::general_purpose::STANDARD.encode(value.trim().as_bytes()));
        }
    };
    view! {
        <fieldset class="card synthetic">
            <legend>{t("face.synthetic.legend")}</legend>
            <p class="hint">{t("face.synthetic.hint")}</p>
            <label>
                {t("face.synthetic.phrase")}
                <input type="text" autocomplete="off" data-testid="face-phrase" bind:value=phrase/>
            </label>
            <button type="button" data-testid="face-synthetic" on:click=use_face
                disabled=move || disabled.get() || phrase.get().trim().is_empty()>
                {t("face.synthetic.use")}
            </button>
        </fieldset>
    }
}

#[component]
fn CameraFace(
    deliver: impl Fn(String) + Copy + Send + Sync + 'static,
    disabled: Signal<bool>,
) -> impl IntoView {
    let state = RwSignal::new(CameraState::Idle);
    let video = NodeRef::<leptos::html::Video>::new();
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    #[cfg(feature = "hydrate")]
    let stream = StoredValue::new_local(None::<web_sys::MediaStream>);

    let start = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let Some(el) = video.get_untracked() else {
                return;
            };
            state.set(CameraState::Starting);
            spawn_local(async move {
                match camera::open(&el).await {
                    Ok(s) => {
                        stream.set_value(Some(s));
                        state.set(CameraState::Live);
                    }
                    Err(e) => state.set(e),
                }
            });
        }
    };
    let capture = move |_| {
        #[cfg(feature = "hydrate")]
        {
            let (Some(v), Some(c)) = (video.get_untracked(), canvas.get_untracked()) else {
                return;
            };
            match camera::capture(&v, &c) {
                Some(image) => {
                    if let Some(s) = stream.get_value() {
                        camera::stop(&s);
                    }
                    stream.set_value(None);
                    state.set(CameraState::Idle);
                    deliver(image);
                }
                None => state.set(CameraState::Failed),
            }
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = deliver;
    };
    #[cfg(feature = "hydrate")]
    on_cleanup(move || {
        if let Some(s) = stream.try_get_value().flatten() {
            camera::stop(&s);
        }
    });

    let message = move || match state.get() {
        CameraState::Denied => Some(t("face.camera.denied")),
        CameraState::Unsupported => Some(t("face.camera.unsupported")),
        CameraState::Timeout => Some(t("face.camera.timeout")),
        CameraState::Failed => Some(t("face.camera.failed")),
        _ => None,
    };
    view! {
        <div class="card camera">
            <p class="hint">{t("face.camera.hint")}</p>
            <video node_ref=video autoplay=true playsinline=true muted=true
                class:hidden=move || state.get() != CameraState::Live aria-label=t("face.camera.preview")></video>
            <canvas node_ref=canvas class="hidden"></canvas>
            <div class="row">
                <Show when=move || state.get() != CameraState::Live>
                    <button type="button" data-testid="camera-start" on:click=start
                        disabled=move || disabled.get() || state.get() == CameraState::Starting>
                        {t("face.camera.start")}
                    </button>
                </Show>
                <Show when=move || state.get() == CameraState::Live>
                    <button type="button" data-testid="camera-capture" on:click=capture disabled=move || disabled.get()>
                        {t("face.camera.capture")}
                    </button>
                </Show>
            </div>
            {move || message().map(|m| view! { <p class="notice error" role="alert">{m}</p> })}
        </div>
    }
}

#[cfg(feature = "hydrate")]
mod camera {
    use super::CameraState;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    const TIMEOUT_MS: i32 = 15_000;

    pub async fn open(
        video: &web_sys::HtmlVideoElement,
    ) -> Result<web_sys::MediaStream, CameraState> {
        let window = web_sys::window().ok_or(CameraState::Unsupported)?;
        let devices = window
            .navigator()
            .media_devices()
            .map_err(|_| CameraState::Unsupported)?;
        let constraints = web_sys::MediaStreamConstraints::new();
        constraints.set_video_bool(true);
        let request = devices
            .get_user_media_with_constraints(&constraints)
            .map_err(|_| CameraState::Unsupported)?;
        let timeout = js_sys::Promise::new(&mut |_, reject| {
            let _ =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&reject, TIMEOUT_MS);
        });
        let raced = js_sys::Promise::race(&js_sys::Array::of2(&request, &timeout));
        let value = JsFuture::from(raced).await.map_err(|e| classify(&e))?;
        let stream: web_sys::MediaStream = value.dyn_into().map_err(|_| CameraState::Failed)?;
        video.set_src_object(Some(&stream));
        let _ = video.play();
        Ok(stream)
    }

    fn classify(error: &JsValue) -> CameraState {
        if error.is_undefined() {
            return CameraState::Timeout;
        }
        match error.dyn_ref::<web_sys::DomException>().map(|e| e.name()) {
            Some(name) if name == "NotAllowedError" || name == "SecurityError" => {
                CameraState::Denied
            }
            Some(name) if name == "NotFoundError" || name == "OverconstrainedError" => {
                CameraState::Unsupported
            }
            _ => CameraState::Failed,
        }
    }

    /// JPEG frame as base64 (without the data URL prefix).
    pub fn capture(
        video: &web_sys::HtmlVideoElement,
        canvas: &web_sys::HtmlCanvasElement,
    ) -> Option<String> {
        let (w, h) = (video.video_width(), video.video_height());
        if w == 0 || h == 0 {
            return None;
        }
        canvas.set_width(w);
        canvas.set_height(h);
        let ctx: web_sys::CanvasRenderingContext2d =
            canvas.get_context("2d").ok()??.dyn_into().ok()?;
        ctx.draw_image_with_html_video_element(video, 0.0, 0.0)
            .ok()?;
        let url = canvas
            .to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.85))
            .ok()?;
        url.split_once(',').map(|(_, b64)| b64.to_owned())
    }

    pub fn stop(stream: &web_sys::MediaStream) {
        for track in stream.get_tracks().iter() {
            if let Ok(track) = track.dyn_into::<web_sys::MediaStreamTrack>() {
                track.stop();
            }
        }
    }
}
