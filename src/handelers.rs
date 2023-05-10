use axum::http::{StatusCode, Uri};
use tokio::signal;
use tracing::error;
use tracing::info;

#[cfg(not(tarpaulin_include))]
///handle the shutdown signal
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("signal received, starting graceful shutdown");
}

#[cfg(not(tarpaulin_include))]
///handle fallback
pub async fn fallback(uri: Uri) -> (StatusCode, String) {
    error!("route not found: {uri}");
    (StatusCode::NOT_FOUND, format!("No route for {uri}"))
}
