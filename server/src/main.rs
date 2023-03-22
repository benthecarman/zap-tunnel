mod config;
mod models;
mod subscribers;

use axum::response::Html;
use axum::routing::get;
use axum::{Extension, Router};
use clap::Parser;
use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::SqliteConnection;
use diesel_migrations::MigrationHarness;
use dioxus::prelude::*;
use std::time::Duration;

use crate::config::*;
use crate::models::MIGRATIONS;
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

    // DB management
    let manager = ConnectionManager::<SqliteConnection>::new(config.db_path);
    let pool = Pool::builder()
        .max_size(16)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(30)),
        }))
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool");
    let connection = &mut pool.get().unwrap();
    connection
        .run_pending_migrations(MIGRATIONS)
        .expect("migrations could not run");

    let client_router = client.router().clone();

    // HTLC event stream
    println!("Starting htlc event subscription");
    let client_router_htlc_event = client_router.clone();

    spawn(async move { start_htlc_event_subscription(client_router_htlc_event).await });

    // HTLC interceptor
    println!("Starting HTLC interceptor");
    spawn(async move { start_htlc_interceptor(client_router).await });

    let addr: std::net::SocketAddr = format!("{}:{}", config.bind, config.port)
        .parse()
        .expect("Failed to parse bind/port for webserver");

    println!("Webserver running on http://{}", addr);

    let router = Router::new().route("/", get(index)).layer(Extension(state));

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

#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionOptions
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
