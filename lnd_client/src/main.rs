use ::config::FileFormat;
use bitcoin::hashes::{sha256, Hash, HashEngine, Hmac, HmacEngine};
use bitcoin::secp256k1::{All, Secp256k1, SecretKey};
use clap::Parser;
use tonic_openssl_lnd::lnrpc::SignMessageRequest;
use tonic_openssl_lnd::LndLightningClient;

use crate::config::{default_cert_file, default_macaroon_file, Config};

mod api;
mod app;
mod config;
mod error_template;
mod fileserv;
mod models;

#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct State {
    pub context: Secp256k1<All>,
    pub config: Config,
    pub lnd: LndLightningClient,
    hashing_key: sha256::Hash,
}

impl State {
    pub(crate) fn get_secret_key(&self, proxy_url: &str) -> anyhow::Result<SecretKey> {
        let mut engine = HmacEngine::<sha256::Hash>::new(&self.hashing_key);
        let url = url::Url::parse(proxy_url)?;
        let host = url.host().ok_or(anyhow::anyhow!("No host"))?;
        engine.input(host.to_string().as_bytes());
        let bytes = Hmac::<sha256::Hash>::from_engine(engine).into_inner();
        Ok(SecretKey::from_slice(&bytes)?)
    }
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use crate::app::*;
    use crate::fileserv::*;
    use axum::routing::{get, post};
    use axum::{Extension, Router};
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};

    register_server_functions();

    // Setting this to None means we'll be using cargo-leptos and its env vars
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(|cx| view! { cx, <App/> }).await;

    let state = init().await.expect("failed to init state");

    // build our application with a route
    let app = Router::new()
        .route("/api/*fn_name", post(leptos_axum::handle_server_fns))
        .route("/all", get(api::get_all))
        .route("/setup-user", post(api::setup_user))
        .route("/status/:proxy", get(api::view_status))
        .leptos_routes(&leptos_options, routes, |cx| view! { cx, <App/> })
        .fallback(file_and_error_handler)
        .with_state(leptos_options)
        .layer(Extension(state.clone()));

    // spawn a task to run the run loop
    let handle = tokio::spawn(async move {
        api::run_loop(state).await.expect("failed to run run_loop");
    });

    // run our app with hyper
    log!("listening on http://{}", &addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    handle.await.expect("failed to join run_loop");
}

const LUD_13_STRING: &str = "DO NOT EVER SIGN THIS TEXT WITH YOUR PRIVATE KEYS! IT IS ONLY USED FOR DERIVATION OF LNURL-AUTH HASHING-KEY, DISCLOSING ITS SIGNATURE WILL COMPROMISE YOUR LNURL-AUTH IDENTITY AND MAY LEAD TO LOSS OF FUNDS!";

#[cfg(feature = "ssr")]
async fn init() -> anyhow::Result<State> {
    let _cmd: Config = Config::parse();

    let from_file: Config = ::config::Config::builder()
        .add_source(::config::File::with_name("config.toml").format(FileFormat::Toml))
        .build()?
        .try_deserialize()?;

    // let config = cmd.combine(from_file);
    let config = from_file;
    let config_clone: Config = config.clone();

    if sled::open(&config.db_path).is_err() {
        std::fs::create_dir_all(&config.db_path)?;
    }

    let macaroon_file = config
        .macaroon_file
        .clone()
        .unwrap_or_else(|| default_macaroon_file(&config.network));

    let cert_file = config.cert_file.unwrap_or_else(default_cert_file);

    let mut lnd_client =
        tonic_openssl_lnd::connect(config.lnd_host, config.lnd_port, cert_file, macaroon_file)
            .await
            .expect("failed to connect");

    let req = SignMessageRequest {
        msg: LUD_13_STRING.as_bytes().to_vec(),
        ..Default::default()
    };
    let sig = lnd_client
        .lightning()
        .clone()
        .sign_message(req)
        .await
        .expect("failed to sign")
        .into_inner();

    let hashing_key = sha256::Hash::hash(&Vec::<u8>::from(sig.signature));

    let context = Secp256k1::new();

    Ok(State {
        context,
        config: config_clone,
        lnd: lnd_client.lightning().clone(),
        hashing_key,
    })
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
