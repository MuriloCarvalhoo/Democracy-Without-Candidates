//! Real transport of the application: SSR pages and Leptos server functions
//! (`POST /api/<endpoint>`, form-encoded input, JSON output) with the Origin header
//! required by the CSRF check. A request only counts as a success when the
//! application response is valid, not merely HTTP 2xx.

use goose::prelude::*;

/// GET a page/endpoint and require `needle` in a 2xx body.
pub async fn get_expect(
    user: &mut GooseUser,
    name: &str,
    path: &str,
    needle: &str,
) -> TransactionResult {
    let request = GooseRequest::builder().path(path).name(name).build();
    let mut goose = user.request(request).await?;
    if let Ok(response) = goose.response {
        let status = response.status();
        let headers = response.headers().clone();
        match response.text().await {
            Ok(body) if status.is_success() && body.contains(needle) => {}
            Ok(body) => {
                return user.set_failure(
                    &format!("{name}: unexpected response {status}"),
                    &mut goose.request,
                    Some(&headers),
                    Some(&body),
                );
            }
            Err(e) => {
                return user.set_failure(
                    &format!("{name}: body {e}"),
                    &mut goose.request,
                    Some(&headers),
                    None,
                );
            }
        }
    }
    Ok(())
}

/// Calls a server function; returns the decoded JSON value on success. Transport
/// errors are already recorded as failures by Goose; invalid answers are marked here.
pub async fn call(
    user: &mut GooseUser,
    endpoint: &str,
    form_body: &str,
) -> Result<Option<serde_json::Value>, Box<TransactionError>> {
    let origin = user.base_url.origin().ascii_serialization();
    let builder = user
        .get_request_builder(&GooseMethod::Post, &format!("/api/{endpoint}"))?
        .header("Origin", origin)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(form_body.to_owned());
    let request = GooseRequest::builder()
        .method(GooseMethod::Post)
        .name(endpoint)
        .set_request_builder(builder)
        .build();
    let mut goose = user.request(request).await?;
    let Ok(response) = goose.response else {
        return Ok(None);
    };
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.text().await.unwrap_or_default();
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) if status.is_success() => Ok(Some(value)),
        _ => {
            user.set_failure(
                &format!("{endpoint}: {status}"),
                &mut goose.request,
                Some(&headers),
                Some(&body),
            )?;
            Ok(None)
        }
    }
}
