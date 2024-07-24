use axum::http::{StatusCode, Uri};
use stream_cancel::{Trigger, Tripwire};
use tokio::signal;
use tracing::{error, info};

#[cfg(not(tarpaulin_include))]
/// This function hold a channel, when a shut down signal is intercepted it drop
/// this channel completing a future that trigger the graceful shutdown of the app.
pub async fn shutdown_signal_trigger(trigger: Trigger) {
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
        () = ctrl_c => {},
        () = terminate => {},
    }

    info!("signal received, starting graceful shutdown");
    drop(trigger);
}

/// This function await for the message to complete to trigger the graceful shutdown.
#[cfg(not(tarpaulin_include))]
pub async fn shutdown_signal(shutdown: Tripwire) {
    shutdown.await;
}

#[cfg(not(tarpaulin_include))]
/// Handle fallback uri.
pub async fn fallback(uri: Uri) -> (StatusCode, String) {
    error!("route not found: {uri}");
    (StatusCode::NOT_FOUND, format!("No route for {uri}"))
}
