use axum::{
    extract::Extension,
    middleware,
    routing::{get},
    Router,
};
use chrono::Local;
use clap::{crate_name, Parser};
use env_logger::{Builder, Target};
use log::LevelFilter;
use std::io::Write;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;
use tower::ServiceBuilder;
use tokio::net::TcpListener;

mod allocator;
mod error;
mod handlers;
mod https;
mod metrics;
mod proxy;
mod state;

use crate::metrics::{track_metrics};
use handlers::{handler_404, health, metrics, root};
use state::State;

#[derive(Parser, Debug, Clone)]
#[command(author = "", version, about = crate_name!())]
pub struct Args {
    /// Port to listen on
    #[arg(short = 'P', long, default_value_t = 8080, env = "ECE_PORT")]
    port: u16,

    /// ECE Username
    #[arg(short = 'u', long, env = "ECE_USERNAME", required_unless_present = "apikey")]
    username: Option<String>,

    /// ECE Password
    #[arg(short = 'p', long, env = "ECE_PASSWORD", required_unless_present = "apikey")]
    password: Option<String>,

    /// ECE API Key
    #[arg(short = 'a', long, env = "ECE_APIKEY", conflicts_with = "username")]
    apikey: Option<String>,

    /// ECE Base URL
    #[arg(short = 'U', long, env = "ECE_URL")]
    url: String,

    /// Default global timeout
    #[arg(short = 't', long, default_value_t = 60, env = "ECE_TIMEOUT")]
    timeout: u64,

    /// Elastic cost per ERU
    #[arg(short = 'e', long, default_value_t = 6000.0, env = "ECE_ERU_COST")]
    eru_cost: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    // Initialize log Builder
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{{\"date\": \"{}\", \"level\": \"{}\", \"log\": {}}}",
                Local::now().format("%Y-%m-%dT%H:%M:%S:%f"),
                record.level(),
                record.args()
            )
        })
        .target(Target::Stdout)
        .filter_level(LevelFilter::Info)
        .parse_default_env()
        .init();

    // Create state for axum
    let state = State::new(args.clone()).await?;

    // Create prometheus handle
    // let recorder_handle = setup_metrics_recorder();

    // These should be authenticated
    let base = Router::new().route("/", get(root));

    // These should NOT be authenticated
    let standard = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics));

    let app = Router::new()
        .merge(base)
        .merge(standard)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .route_layer(middleware::from_fn(track_metrics))
        .fallback(handler_404)
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = TcpListener::bind(addr).await.unwrap();

    log::info!("Listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
