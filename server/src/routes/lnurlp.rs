use anyhow::anyhow;
use std::collections::HashMap;
use std::str::FromStr;

use crate::config::Config;
use crate::models::invoice::{Invoice, DEFAULT_INVOICE_EXPIRY};
use crate::models::zap::Zap;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::hashes::{sha256, Hash};
use diesel::SqliteConnection;
use lightning_invoice::Bolt11Invoice;
use lnurl::pay::{LnURLPayInvoice, PayResponse};
use lnurl::Tag;
use nostr::Event;
use serde_json::json;
use tonic_openssl_lnd::invoicesrpc::AddHoldInvoiceRequest;
use tonic_openssl_lnd::LndInvoicesClient;

use crate::State;

fn calculate_metadata(username: &str, public_url: &str) -> String {
    format!(
        "[[\"text/plain\", \"Pay to {}\"], [\"text/identifier\", \"{}@{}\"]]",
        username, username, public_url
    )
}

pub(crate) fn get_lnurlp_impl(
    username: String,
    config: &Config,
    connection: &mut SqliteConnection,
) -> Option<PayResponse> {
    let metadata = calculate_metadata(&username, &config.public_url);
    let _ = crate::models::user::User::get_by_username(connection, &username)?;
    let callback = format!("https://{}/lnurlp/{}", config.public_url, username);
    let max_sendable = 100_000_000;
    let min_sendable = config.min_sendable();

    Some(PayResponse {
        callback,
        max_sendable,
        min_sendable,
        tag: Tag::PayRequest,
        metadata,
        allows_nostr: Some(true),
        nostr_pubkey: Some(config.public_key()),
    })
}

pub async fn get_lnurlp(
    Path(username): Path<String>,
    Extension(state): Extension<State>,
) -> Result<Json<PayResponse>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database connection"),
        )
    })?;

    match get_lnurlp_impl(username, &state.config, &mut connection) {
        Some(res) => Ok(Json(res)),
        None => Err((StatusCode::NOT_FOUND, String::from("{\"status\":\"ERROR\",\"reason\":\"The user you're searching for could not be found.\"}"))),
    }
}

pub(crate) async fn get_lnurl_invoice_impl(
    username: String,
    amount_msats: u64,
    zap_request: Option<Event>,
    invoice_client: &LndInvoicesClient,
    config: &Config,
    connection: &mut SqliteConnection,
) -> anyhow::Result<Option<Bolt11Invoice>> {
    if amount_msats <= config.min_sendable() {
        return Err(anyhow!("Amount too small"));
    }

    let desc_hash = match zap_request.as_ref() {
        None => {
            let metadata = calculate_metadata(&username, &config.public_url);
            sha256::Hash::hash(metadata.as_bytes())
        }
        Some(event) => {
            // todo validate as valid zap request
            if event.kind != nostr::Kind::ZapRequest {
                return Err(anyhow!("Invalid zap request"));
            }
            sha256::Hash::hash(event.as_json().as_bytes())
        }
    };

    let invoice_db = match Invoice::get_next_invoice(&username, connection) {
        Err(e) => {
            println!("Error getting invoice: {}", e);
            return Ok(None);
        }
        Ok(db) => db,
    };

    let cltv_expiry = invoice_db.invoice().min_final_cltv_expiry_delta() * 6 + 3;

    if cltv_expiry > 2016 {
        return Err(anyhow!("CLTV expiry too long"));
    }

    let request = AddHoldInvoiceRequest {
        hash: invoice_db.payment_hash().to_vec(),
        value_msat: amount_msats as i64,
        description_hash: desc_hash.to_vec(),
        expiry: DEFAULT_INVOICE_EXPIRY,
        cltv_expiry,
        ..Default::default()
    };

    let client = &mut invoice_client.clone();

    let resp = client.add_hold_invoice(request).await?.into_inner();
    let inv = Bolt11Invoice::from_str(&resp.payment_request)?;

    if let Some(zap_request) = zap_request {
        let zap = Zap::new(&inv, zap_request, None);
        Zap::create(zap, connection)?;
    }

    Ok(Some(inv))
}

pub async fn get_lnurl_invoice(
    Path(username): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    Extension(state): Extension<State>,
) -> Result<Json<LnURLPayInvoice>, (StatusCode, Json<serde_json::Value>)> {
    match params.get("amount").and_then(|a| a.parse::<u64>().ok()) {
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "ERROR",
                "reason": "Missing amount parameter",
            })),
        )),
        Some(amount_msats) => {
            let zap_request = params.get("nostr").map_or_else(
                || Ok(None),
                |event_str| {
                    Event::from_json(event_str)
                        .map_err(|_| {
                            (
                                StatusCode::BAD_REQUEST,
                                Json(json!({
                                    "status": "ERROR",
                                    "reason": "Invalid zap request",
                                })),
                            )
                        })
                        .map(Some)
                },
            )?;

            let mut connection = state.db_pool.get().map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "status": "ERROR",
                        "reason": "Failed to get database connection",
                    })),
                )
            })?;

            let res = get_lnurl_invoice_impl(
                username,
                amount_msats,
                zap_request,
                &state.invoice_client,
                &state.config,
                &mut connection,
            )
            .await;

            match res {
                Ok(Some(inv)) => {
                    println!("Generated invoice: {}", inv);
                    let res = LnURLPayInvoice::new(inv);
                    Ok(Json(res))
                }
                Ok(None) => Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "status": "ERROR",
                        "reason": "The user you're searching for could not be found."
                    })),
                )),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "status": "ERROR",
                        "reason": format!("Failed to generate invoice: {}", e)
                    })),
                )),
            }
        }
    }
}
