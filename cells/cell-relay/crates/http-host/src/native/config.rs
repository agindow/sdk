use agw_guard::statistical::Operation;
use serde::Deserialize;
use std::{collections::BTreeMap, net::SocketAddr};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub url: String,
    pub method: String,
    pub operation: Operation,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub selection: serde_json::Value,
}
pub struct Settings {
    pub bind: SocketAddr,
    pub socket: Option<String>,
    pub instance: String,
    pub routes: BTreeMap<String, Route>,
    pub work: usize,
}
impl Settings {
    pub fn from_env() -> Result<Self, String> {
        let bind: SocketAddr = std::env::var("AGINDOW_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8789".into())
            .parse()
            .map_err(|_| "invalid AGINDOW_BIND")?;
        if !bind.ip().is_loopback() {
            return Err("sidecar must bind a loopback address".into());
        }
        let json =
            std::env::var("AGINDOW_CONNECTIONS").map_err(|_| "AGINDOW_CONNECTIONS is required")?;
        if json.len() > 4 * 1024 * 1024 {
            return Err("connection configuration exceeds bound".into());
        }
        let routes: BTreeMap<String, Route> =
            serde_json::from_str(&json).map_err(|_| "invalid AGINDOW_CONNECTIONS")?;
        if routes.is_empty() || routes.len() > 256 {
            return Err("configure 1–256 explicit connection/tool routes".into());
        }
        for (key, route) in &routes {
            let url = reqwest::Url::parse(&route.url).map_err(|_| "invalid upstream URL")?;
            if !["http", "https"].contains(&url.scheme())
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || key.split('/').count() != 2
                || route.headers.len() > 32
            {
                return Err("invalid connection route".into());
            }
            reqwest::Method::from_bytes(route.method.as_bytes())
                .map_err(|_| "invalid upstream method")?;
            for (name, value) in &route.headers {
                reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| "invalid bound header")?;
                reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| "invalid bound header")?;
                if ["host", "content-length", "transfer-encoding", "connection"]
                    .contains(&name.to_ascii_lowercase().as_str())
                {
                    return Err("framing headers cannot be configured".into());
                }
            }
            if !route.selection.is_null() {
                let mut parts = key.split('/');
                if route.selection["connection"].as_str() != parts.next()
                    || route.selection["operation"].as_str() != parts.next()
                {
                    return Err("selection must match route scope".into());
                }
            }
        }
        let instance =
            std::env::var("AGINDOW_INSTANCE_ID").unwrap_or_else(|_| "local-sidecar".into());
        if instance.is_empty()
            || instance.len() > 128
            || !instance
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("invalid instance id".into());
        }
        let work = std::env::var("AGINDOW_WORK")
            .unwrap_or_else(|_| "200000000".into())
            .parse()
            .map_err(|_| "invalid work allowance")?;
        Ok(Self {
            bind,
            socket: std::env::var("AGINDOW_SOCKET").ok(),
            instance,
            routes,
            work,
        })
    }
}
