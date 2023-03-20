use std::sync::Arc;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router, Server,
};

use tokio::{sync::RwLock, task::JoinHandle};
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

use rs_utils::config::{init_watcher, Config};

pub mod permission {
    tonic::include_proto!("permission");
}

mod controler;
mod handelers;
use handelers::shutdown_signal;
mod router;
use router::{alive, ready, update};
mod config;
use config::{SiriusConfig, CONFIG_FALLBACK};
mod error;

type ConfigState = Arc<RwLock<SiriusConfig>>;

///main router config
pub fn app(shared_state: ConfigState) -> Router {
    info!("configuring main router");
    Router::new()
        .route("/api/iam/roles", post(update))
        .with_state(shared_state)
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

///heatlh router config
pub fn health(shared_state: ConfigState) -> Router {
    info!("configuring health router");
    Router::new()
        .route("/alive", get(alive))
        .route("/ready", get(ready))
        .with_state(shared_state)
}

///launch http router
async fn make_http(
    shared_state: ConfigState,
    f: fn(ConfigState) -> Router,
    addr: String,
) -> Result<JoinHandle<Result<(), hyper::Error>>> {
    //todo: add path for tlscertificate
    let handle = tokio::spawn(
        Server::bind(&addr.parse().unwrap())
            .serve(f(shared_state).into_make_service())
            .with_graceful_shutdown(shutdown_signal()),
    );
    info!("lauching http server on: {addr}");
    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "DEBUG");
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

    info!("statrting http router");
    let http_addr = service.addr.clone() + ":" + &service.ports.main as &str;
    let http = make_http(shared_state.clone(), app, http_addr).await?;

    let health_addr = service.addr.clone() + ":" + &service.ports.health as &str;
    let health = make_http(shared_state.clone(), health, health_addr).await?;
    let (http_critical, health_critical) = tokio::try_join!(http, health)?;
    http_critical?;
    health_critical?;
    Ok(())
}
