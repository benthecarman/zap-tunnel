use crate::models::*;
use crate::State;
use anyhow::anyhow;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::{Extension, Form, Json};
use lightning_invoice::Invoice as LnInvoice;
use std::str::FromStr;
use std::time::SystemTime;
use std::{thread, time};
use tonic_openssl_lnd::lnrpc;
use zap_tunnel_client::blocking::*;
use zap_tunnel_client::Builder;

pub(crate) async fn run_loop(state: State) -> anyhow::Result<()> {
    loop {
        let keys: Vec<String> = {
            let db: sled::Db = sled::open(&state.config.db_path)?;
            db.iter()
                .filter(|x| x.is_ok())
                .map(|x| String::from_utf8(x.unwrap().0.to_vec()))
                .filter(|x| x.is_ok())
                .map(|x| x.unwrap())
                .collect()
        };
        let mut futures = Vec::new();
        for key in keys.iter() {
            let client = BlockingClient::from_builder(Builder::new(key))?;
            let fut = upload_invoices(&state, client);
            futures.push(fut);
        }
        let combined_futures = futures::future::join_all(futures);
        let _ = combined_futures.await;
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

    // use i64 to handle negatives safely
    let need_invoices = state.config.invoice_cache as i64 - invoices_remaining as i64;

    if need_invoices > 0 {
        let mut invoices: Vec<LnInvoice> = vec![];
        for _ in 0..need_invoices {
            let inv = lnrpc::Invoice {
                memo: state.config.invoice_memo.clone(),
                expiry: 31536000, // ~1 year
                private: true,
                ..Default::default()
            };
            let invoice = state.lnd.clone().add_invoice(inv).await?.into_inner();
            let ln_invoice = LnInvoice::from_str(&invoice.payment_request)?;

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
            let url = format!("https://{}/", payload.proxy);
            let client = BlockingClient::from_builder(Builder::new(&url))?;
            let key = state.get_secret_key(&url)?;
            let resp = client.create_user(&state.context, &payload.username, &key)?;
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
) -> Result<Json<bool>, (StatusCode, String)> {
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
        Ok(_) => Ok(Json(true)),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

async fn view_status_impl(state: &State, proxy: &str) -> anyhow::Result<Status> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let client = BlockingClient::from_builder(Builder::new(proxy))?;
    let key = state.get_secret_key(proxy)?;
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
