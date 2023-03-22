mod config;
mod subscribers;

use axum::response::Html;
use axum::routing::get;
use axum::{Extension, Router};
use clap::Parser;
use dioxus::prelude::*;

use crate::config::*;
use crate::subscribers::*;
use tokio::task::spawn;
use tonic_openssl_lnd::lnrpc::{GetInfoRequest, GetInfoResponse};

#[derive(Clone)]
struct State {
    connection_string: String,
}

#[tokio::main]
async fn main() {
    let config: Config = Config::parse();

    let macaroon_file = config
        .macaroon_file
        .unwrap_or_else(|| default_macaroon_file(config.network));
    let cert_file = config.cert_file.unwrap_or_else(default_cert_file);

    let mut client =
        tonic_openssl_lnd::connect(config.lnd_host, config.lnd_port, cert_file, macaroon_file)
            .await
            .expect("failed to connect");

    let mut ln_client = client.lightning().clone();
    let lnd_info: GetInfoResponse = ln_client
        .get_info(GetInfoRequest {})
        .await
        .expect("Failed to get lnd info")
        .into_inner();

    let state = State {
        connection_string: lnd_info
            .uris
            .first()
            .expect("Lightning node needs a public uri")
            .clone(),
    };

    let client_router = client.router().clone();

    // HTLC event stream
    println!("Starting htlc event subscription");
    let client_router_htlc_event = client_router.clone();

    spawn(async move {
        start_htlc_event_subscription(client_router_htlc_event).await
    });

    // HTLC interceptor
    println!("Starting HTLC interceptor");
    spawn(async move { start_htlc_interceptor(client_router).await });

    let addr: std::net::SocketAddr = format!("{}:{}", config.bind, config.port)
        .parse()
        .expect("Failed to parse bind/port for webserver");

    println!("Webserver running on http://{}", addr);

    let router = Router::new()
        .route("/", get(index))
        .layer(Extension(state));

    let server = axum::Server::bind(&addr).serve(router.into_make_service());

    let graceful = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to create Ctrl+C shutdown signal");
    });

    // Await the server to receive the shutdown signal
    if let Err(e) = graceful.await {
        eprintln!("shutdown error: {}", e);
    }
}

async fn index(Extension(state): Extension<State>) -> Html<String> {
    let connect = format!("Connect with me here: {}", state.connection_string);

    Html(dioxus::ssr::render_lazy(rsx! {
            h1 { "Hello world!" }
            p {"{connect}"}
    }))
}
