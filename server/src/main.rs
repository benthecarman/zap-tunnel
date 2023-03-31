use std::time::Duration;

use axum::http::{StatusCode, Uri};
use axum::routing::{get, post};
use axum::{Extension, Router};
use clap::Parser;
use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::SqliteConnection;
use diesel_migrations::MigrationHarness;
use tokio::task::spawn;
use tonic_openssl_lnd::lnrpc::{GetInfoRequest, GetInfoResponse};
use tonic_openssl_lnd::LndInvoicesClient;

use crate::config::*;
use crate::models::MIGRATIONS;
use crate::routes::index;
use crate::subscriber::*;

mod config;
mod models;
mod nostr;
mod routes;
mod subscriber;

#[derive(Clone)]
pub struct State {
    connection_string: String,
    config: Config,
    invoice_client: LndInvoicesClient,
    db_pool: Pool<ConnectionManager<SqliteConnection>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config: Config = Config::parse();

    let mut client = tonic_openssl_lnd::connect(
        config.lnd_host.clone(),
        config.lnd_port,
        config.cert_file(),
        config.macaroon_file(),
    )
    .await
    .expect("failed to connect");

    let mut ln_client = client.lightning().clone();
    let lnd_info: GetInfoResponse = ln_client
        .get_info(GetInfoRequest {})
        .await
        .expect("Failed to get lnd info")
        .into_inner();

    // DB management
    let manager = ConnectionManager::<SqliteConnection>::new(&config.db_path);
    let db_pool = Pool::builder()
        .max_size(16)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(30)),
        }))
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool");
    let connection = &mut db_pool.get()?;
    connection
        .run_pending_migrations(MIGRATIONS)
        .expect("migrations could not run");

    let state = State {
        connection_string: lnd_info
            .uris
            .first()
            .expect("Lightning node needs a public uri")
            .clone(),
        config: config.clone(),
        invoice_client: client.invoices().clone(),
        db_pool: db_pool.clone(),
    };

    let lightning_client = client.lightning().clone();
    let router_client = client.router().clone();
    let invoice_client = client.invoices().clone();

    // Invoice event stream
    spawn(start_invoice_subscription(
        lightning_client,
        router_client,
        invoice_client,
        config.clone(),
        db_pool,
    ));

    let addr: std::net::SocketAddr = format!("{}:{}", config.bind, config.port)
        .parse()
        .expect("Failed to parse bind/port for webserver");

    println!("Webserver running on http://{}", addr);

    let server_router = Router::new()
        .route("/", get(index))
        .route("/create-user", post(routes::create_user))
        .route("/check-user", get(routes::check_user))
        .route("/.well-known/lnurlp/:username", get(routes::get_lnurlp))
        .route("/lnurlp/:username", get(routes::get_lnurl_invoice))
        .route("/add-invoices", post(routes::add_invoices))
        .fallback(fallback)
        .layer(Extension(state));

    let server = axum::Server::bind(&addr).serve(server_router.into_make_service());

    let graceful = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to create Ctrl+C shutdown signal");
    });

    // Await the server to receive the shutdown signal
    if let Err(e) = graceful.await {
        eprintln!("shutdown error: {}", e);
    }

    Ok(())
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

async fn fallback(uri: Uri) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, format!("No route for {}", uri))
}
