use axum::{
    handler::Handler,
    routing::{get},
    Router,
    middleware,
    extract::Extension
};
use chrono::Local;
use clap::{crate_name, crate_version, Command, Arg};
use env_logger::{Builder, Target};
use log::LevelFilter;
use std::io::Write;
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

mod error;
mod handlers;
mod https;
mod metrics;
mod state;
mod allocator;
mod proxy;

use crate::metrics::{setup_metrics_recorder, track_metrics};
use handlers::{handler_404, health, root, metrics};
use state::State;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opts = Command::new(crate_name!())
        .version(crate_version!())
        .author("")
        .about(crate_name!())
        .arg(
            Arg::new("port")
                .short('P')
                .long("port")
                .help("Set port to listen on")
                .env("ECE_PORT")
                .default_value("8080")
                .takes_value(true),
        )
        .arg(
            Arg::new("username")
                .short('u')
                .long("username")
                .help("ECE Username")
                .env("ECE_USERNAME")
                .required_unless_present("apikey")
                .takes_value(true),
        )
        .arg(
            Arg::new("password")
                .short('p')
                .long("password")
                .help("ECE Password")
                .env("ECE_PASSWORD")
                .required_unless_present("apikey")
                .takes_value(true),
        )
        .arg(
            Arg::new("apikey")
                .short('a')
                .long("apikey")
                .help("ECE API Key")
                .env("ECE_APIKEY")
                .required(false)
                .conflicts_with("username")
                .takes_value(true),
        )
        .arg(
            Arg::new("url")
                .short('U')
                .long("url")
                .help("ECE Base URL")
                .env("ECE_URL")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("timeout")
                .short('t')
                .long("timeout")
                .help("Set default global timeout")
                .default_value("60")
                .env("ECE_TIMEOUT")
                .takes_value(true),
        )
        .get_matches();

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

    // Set port
    let port: u16 = opts.value_of("port").unwrap().parse().unwrap_or_else(|_| {
        eprintln!("specified port isn't in a valid range, setting to 8080");
        8080
    });

    // Create state for axum
    let state = State::new(opts.clone()).await?;

    // Create prometheus handle
    let recorder_handle = setup_metrics_recorder();

    // These should be authenticated
    let base = Router::new()
        .route("/", get(root));

    // These should NOT be authenticated
    let standard = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics));

    let app = Router::new()
        .merge(base)
        .merge(standard)
        .layer(TraceLayer::new_for_http())
        .route_layer(middleware::from_fn(track_metrics))
        .layer(Extension(recorder_handle))
        .layer(Extension(state));

    // add a fallback service for handling routes to unknown paths
    let app = app.fallback(handler_404.into_service());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
