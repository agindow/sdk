//! TI-33 native host: loopback and UDS, explicit connection handles, no MITM.
mod archive;
mod config;
mod handler;
use agw_guard::statistical::Engine;
use axum::{Router, routing::get, routing::post};
pub use config::{Route, Settings};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicU64},
};

pub struct State {
    pub settings: Settings,
    pub client: reqwest::Client,
    pub engines: BTreeMap<String, Engine>,
    pub calls: AtomicU64,
    pub archive: std::sync::Mutex<archive::Archive>,
}
impl State {
    pub fn new(settings: Settings) -> Result<Arc<Self>, String> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let mut engines = BTreeMap::new();
        for (key, route) in &settings.routes {
            engines.insert(
                key.clone(),
                agw_node_guest::local_engine(&route.selection.to_string())?,
            );
        }
        Ok(Arc::new(Self {
            settings,
            client,
            engines,
            calls: AtomicU64::new(0),
            archive: std::sync::Mutex::new(archive::Archive::default()),
        }))
    }
}
pub fn router(state: Arc<State>) -> Router {
    Router::new()
        .route("/healthz", get(handler::health))
        .route("/v1/registration", get(handler::registration))
        .route("/v1/guard", post(handler::guard))
        .route("/v1/audit/{id}", get(handler::audit))
        .route("/v1/outcome/{id}", get(handler::outcome))
        .layer(axum::extract::DefaultBodyLimit::max(
            8 * 1024 * 1024 + 65536,
        ))
        .layer(axum::middleware::from_fn(local_origin))
        .with_state(state)
}

/// Local execution handles are not ambient authority for arbitrary web origins.
async fn local_origin(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let host = request.headers().get("host").and_then(|v| v.to_str().ok());
    let local = host
        .and_then(|host| reqwest::Url::parse(&format!("http://{host}")).ok())
        .is_some_and(|url| {
            url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            }) && url.username().is_empty()
                && url.password().is_none()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
        });
    let origin = request.headers().get("origin");
    let same_origin = origin.is_none_or(|origin| {
        origin
            .to_str()
            .ok()
            .is_some_and(|origin| host.is_some_and(|host| origin == format!("http://{host}")))
    });
    if !local || !same_origin {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}
