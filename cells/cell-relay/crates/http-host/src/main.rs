use agw_http_host::native::{Settings, State, router};
use std::{
    error::Error,
    fs,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::Path,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let settings = Settings::from_env()?;
    let listener = tokio::net::TcpListener::bind(settings.bind).await?;
    let socket = if let Some(path) = &settings.socket {
        // Never unlink an existing socket: it may belong to a live instance.
        // Operators remove stale paths explicitly after checking ownership.
        if Path::new(path).exists() {
            return Err("configured Unix socket already exists".into());
        }
        let listener = tokio::net::UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Some(listener)
    } else {
        None
    };
    let state = State::new(settings)?;
    let application = router(state.clone());
    println!(
        "{}",
        serde_json::json!({ "event": "registered", "instance": state.settings.instance, "address": listener.local_addr()?.to_string() })
    );
    if let Some(socket) = socket {
        let socket_path = state.settings.socket.clone();
        tokio::select! {
            result = axum::serve(listener, application.clone()) => { result?; }
            result = axum::serve(socket, application) => { result?; }
            _ = shutdown() => {}
        }
        if let Some(path) = socket_path
            && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_socket())
        {
            fs::remove_file(path)?;
        }
    } else {
        axum::serve(listener, application)
            .with_graceful_shutdown(shutdown())
            .await?;
    }
    Ok(())
}

async fn shutdown() {
    if let Ok(mut terminate) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    } else {
        let _ = tokio::signal::ctrl_c().await;
    }
}
