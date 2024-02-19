use std::{
    future::{Future, IntoFuture},
    sync::Arc,
};

use anyhow::Result;
use axum::{
    http::HeaderName,
    routing::{get, post},
    serve, Router,
};
use stream_cancel::Tripwire;
use tokio::{net::TcpListener, sync::RwLock, task::JoinHandle};
use tower_http::{
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{info, warn, Level};
use tracing_subscriber::{fmt, EnvFilter};

use rs_utils::config::{init_watcher, Config};

pub mod permission {
    tonic::include_proto!("permission");
}

mod controller;
mod handelers;
use handelers::{fallback, shutdown_signal, shutdown_signal_trigger};
mod router;
use router::{
    alive, list_groups, list_projects, ready, update_groups, update_organisation, update_projects,
};
mod config;
use config::{SiriusConfig, CONFIG_FALLBACK};

use crate::router::list_orga;
mod error;
mod utils;

type ConfigState = Arc<RwLock<SiriusConfig>>;

///main router config
pub fn app(shared_state: ConfigState) -> Router {
    info!("configuring main router");

    let api_route = Router::new()
        .route("/project", post(update_projects).get(list_projects))
        .route("/group", post(update_groups).get(list_groups))
        .route("/organisation", post(update_organisation).get(list_orga));

    Router::new()
        .nest("/api/iam", api_route)
        .with_state(shared_state)
        .fallback(fallback)
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static("correlation_id"),
            MakeRequestUuid,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Micros),
                ),
        )
}

///heatlh router config
pub fn health(shared_state: ConfigState) -> Router {
    info!("configuring health router");
    Router::new()
        .route("/alive", get(alive))
        .route("/ready", get(ready))
        .fallback(fallback)
        .with_state(shared_state)
}

///launch http router
async fn make_http<T>(
    shared_state: ConfigState,
    f: fn(ConfigState) -> Router,
    addr: String,
    signal: T,
) -> JoinHandle<Result<(), std::io::Error>>
where
    T: Future<Output = ()> + std::marker::Send + 'static,
{
    info!("listening on {}", addr);
    let listener = TcpListener::bind(&addr).await.unwrap();
    let service = serve(listener, f(shared_state))
        .with_graceful_shutdown(signal)
        .into_future();
    info!("lauching http server on: {addr}");
    tokio::spawn(service)
}

#[cfg(not(tarpaulin_include))]
#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_target(false)
        .with_level(true)
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
        warn!("Config variable not found switching to fallback");
        CONFIG_FALLBACK.to_owned()
    });
    let config = SiriusConfig::new(&config_path).await;
    let service = config.service.clone();
    let shared_state = Arc::new(RwLock::new(config));
    tokio::spawn(init_watcher(config_path, shared_state.clone(), None));
    let (trigger, shutdown) = Tripwire::new();
    let signal_sender = shutdown_signal_trigger(trigger);
    info!("statrting http router");
    let http_addr = service.addr.clone() + ":" + &service.ports.main as &str;
    let http = make_http(shared_state.clone(), app, http_addr, signal_sender).await;
    let signal_receiver = shutdown_signal(shutdown);
    let health_addr = service.addr.clone() + ":" + &service.ports.health as &str;
    let health = make_http(shared_state.clone(), health, health_addr, signal_receiver).await;
    let (http_critical, health_critical) = tokio::try_join!(http, health)?;
    http_critical?;
    health_critical?;
    Ok(())
}
