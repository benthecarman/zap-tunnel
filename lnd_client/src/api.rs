use crate::models::*;
use crate::State;
use anyhow::anyhow;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Form, Json};
use lightning_invoice::Bolt11Invoice;
use std::str::FromStr;
use std::time::SystemTime;
use std::{thread, time};
use tonic_openssl_lnd::lnrpc;
use zap_tunnel_client::blocking::*;
use zap_tunnel_client::Builder;
use zap_tunnel_client::Error::HttpResponse;

fn create_url(proxy: &str) -> String {
    if proxy.starts_with("http://") || proxy.starts_with("https://") {
        proxy.to_string()
    } else if proxy.contains("localhost:") || proxy.contains("127.0.0.1") {
        format!("http://{proxy}")
    } else {
        format!("https://{proxy}")
    }
}

pub(crate) async fn run_loop(state: State) -> anyhow::Result<()> {
    loop {
        let keys: Vec<String> = {
            let db: sled::Db = sled::open(&state.config.db_path)?;
            db.iter()
                .filter_map(|x| x.ok())
                .map(|(x, _)| String::from_utf8(x.to_vec()))
                .filter_map(|x| x.ok())
                .collect()
        };
        let mut futures = Vec::new();
        for key in keys {
            let url = create_url(&key);
            let client = BlockingClient::from_builder(Builder::new(&url))?;
            let fut = upload_invoices(&state, client);
            futures.push(fut);
        }
        let combined_futures = futures::future::join_all(futures).await;
        for result in combined_futures {
            if let Err(e) = result {
                eprintln!("Failed to upload invoices: {e}");
            }
        }
        thread::sleep(time::Duration::from_secs(60));
    }
}

async fn upload_invoices(state: &State, client: BlockingClient) -> anyhow::Result<usize> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    let key = state.get_secret_key(&client.url)?;

    let invoices_remaining = client
        .check_user(&state.context, now, &key)?
        .invoices_remaining;

    println!("Invoices remaining: {}", invoices_remaining);

    let need_invoices = state
        .config
        .invoice_cache
        .checked_sub(invoices_remaining as usize)
        .unwrap_or_default();

    if need_invoices > 0 {
        println!("Adding {} invoices", need_invoices);
        let mut invoices: Vec<Bolt11Invoice> = vec![];
        for i in 0..need_invoices {
            let inv = lnrpc::Invoice {
                memo: state.config.invoice_memo.clone(),
                expiry: 31536000, // ~1 year
                private: true,
                ..Default::default()
            };
            let invoice = match state.lnd.clone().add_invoice(inv).await {
                Ok(invoice) => invoice.into_inner(),
                Err(e) => {
                    eprintln!("Failed to add invoice: {e}");
                    continue;
                }
            };
            let ln_invoice = Bolt11Invoice::from_str(&invoice.payment_request)?;
            invoices.push(ln_invoice);
        }

        let num = client.add_invoices(&state.context, &key, invoices.as_slice())?;
        println!("Added {} invoices", num);

        return Ok(num);
    }

    Ok(0)
}

async fn setup_user_impl(
    current: Option<String>,
    state: &State,
    payload: SetupUser,
    db: sled::Db,
) -> anyhow::Result<()> {
    match current {
        // handle new registration
        None => {
            let url = create_url(&payload.proxy);
            let client = BlockingClient::from_builder(Builder::new(&url))?;
            let key = state.get_secret_key(&url)?;
            let resp = match client.create_user(&state.context, &payload.username, &key) {
                Ok(resp) => resp,
                Err(HttpResponse(_, str)) => {
                    println!("Failed to create user: {}", str.clone().unwrap_or_default());
                    return Err(anyhow!(
                        "Failed to create user: {}",
                        str.unwrap_or_default()
                    ));
                }
                Err(e) => {
                    println!("Failed to create user: {}", e);
                    return Err(anyhow!("Failed to create user"));
                }
            };
            if resp.username == payload.username {
                db.insert(payload.proxy, payload.username.as_bytes())?;
                Ok(())
            } else {
                Err(anyhow!("Failed to register to proxy {}", payload.proxy))
            }
        }
        // handle already registered
        Some(username) => {
            if username != payload.username {
                Err(anyhow!(
                    "Already registered to proxy {} as {username}",
                    payload.proxy
                ))
            } else {
                Ok(())
            }
        }
    }
}

pub async fn setup_user(
    Extension(state): Extension<State>,
    Form(payload): Form<SetupUser>,
) -> Result<Response, (StatusCode, String)> {
    let db: sled::Db = sled::open(&state.config.db_path).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database connection"),
        )
    })?;
    let current = db.get(&payload.proxy).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database item"),
        )
    })?;

    let current = current.map(|v| String::from_utf8(v.to_vec()).unwrap());
    match setup_user_impl(current, &state, payload, db).await {
        Ok(_) => Ok(Redirect::to("/").into_response()),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

async fn view_status_impl(state: &State, proxy: &str) -> anyhow::Result<Status> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let base_url = create_url(&proxy);

    let client = BlockingClient::from_builder(Builder::new(&base_url))?;
    let key = state.get_secret_key(&base_url)?;
    let resp = client.check_user(&state.context, now, &key)?;
    Ok(Status {
        proxy: String::from(proxy),
        username: resp.username,
        invoices_remaining: resp.invoices_remaining,
    })
}

pub async fn view_status(
    Extension(state): Extension<State>,
    Path(proxy): Path<String>,
) -> Result<Json<Status>, (StatusCode, String)> {
    let db: sled::Db = sled::open(&state.config.db_path).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database connection"),
        )
    })?;
    let current = db.get(&proxy).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database item"),
        )
    })?;

    let current = current.map(|v| String::from_utf8(v.to_vec()).unwrap());

    match current {
        None => Err((
            StatusCode::BAD_REQUEST,
            String::from("Not registered to proxy"),
        )),
        Some(_) => match view_status_impl(&state, &proxy).await {
            Ok(status) => Ok(Json(status)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        },
    }
}

pub async fn get_all(
    Extension(state): Extension<State>,
) -> Result<Json<Vec<SetupUser>>, (StatusCode, String)> {
    let db: sled::Db = sled::open(&state.config.db_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get database connection: {e}"),
        )
    })?;
    let iter = db.iter();

    let mut all = vec![];
    for item in iter {
        let (key, value) = item.map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Failed to get database item"),
            )
        })?;
        let key = String::from_utf8(key.to_vec()).unwrap();
        let value = String::from_utf8(value.to_vec()).unwrap();
        all.push(SetupUser {
            proxy: key,
            username: value,
        });
    }

    Ok(Json(all))
}
