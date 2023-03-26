use anyhow::anyhow;
use std::collections::HashMap;
use std::str::FromStr;

use crate::models::invoice::Invoice;
use crate::models::zap::Zap;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::hashes::{sha256, Hash};
use diesel::SqliteConnection;
use lightning_invoice::Invoice as LnInvoice;
use lnurl::pay::{LnURLPayInvoice, PayResponse};
use lnurl::Tag;
use nostr::key::XOnlyPublicKey;
use nostr::Event;
use tonic_openssl_lnd::invoicesrpc::AddHoldInvoiceRequest;
use tonic_openssl_lnd::LndInvoicesClient;

use crate::State;

fn calculate_metadata(username: &str) -> String {
    // todo change identifier to use correct host
    format!(
        "[[\"text/plain\", \"Pay to {}\"], [\"text/identifier\", \"{}@127.0.0.1:3000\"]]",
        username, username
    )
}

pub(crate) fn get_lnurlp_impl(
    username: String,
    nostr_pubkey: XOnlyPublicKey,
    connection: &mut SqliteConnection,
) -> Option<PayResponse> {
    let metadata = calculate_metadata(&username);
    let _ = crate::models::user::User::get_by_username(connection, &username)?;
    // todo change callback to use correct host and https
    let callback = format!("http://127.0.0.1:3000/lnurlp/{username}");
    let max_sendable = 100_000_000;
    let min_sendable = 1_000;

    Some(PayResponse {
        callback,
        max_sendable,
        min_sendable,
        tag: Tag::PayRequest,
        metadata,
        allows_nostr: Some(true),
        nostr_pubkey: Some(nostr_pubkey),
    })
}

pub async fn get_lnurlp(
    Path(username): Path<String>,
    Extension(state): Extension<State>,
) -> Result<Json<PayResponse>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().unwrap();

    match get_lnurlp_impl(username, state.nostr_pubkey, &mut connection) {
        Some(res) => Ok(Json(res)),
        None => Err((StatusCode::NOT_FOUND, String::from("{\"status\":\"ERROR\",\"reason\":\"The user you're searching for could not be found.\"}"))),
    }
}

pub(crate) async fn get_lnurl_invoice_impl(
    username: String,
    amount_msats: u64,
    zap_request: Option<Event>,
    invoice_client: &LndInvoicesClient,
    connection: &mut SqliteConnection,
) -> anyhow::Result<Option<LnInvoice>> {
    if amount_msats < 1_000 {
        return Err(anyhow!("Amount too small"));
    }

    let desc_hash = match zap_request.as_ref() {
        None => {
            let metadata = calculate_metadata(&username);
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

    let invoice_db = match Invoice::get_next_invoice(username, connection) {
        Err(_) => return Ok(None),
        Ok(db) => db,
    };

    // todo make sure this is safe
    let cltv_expiry = invoice_db.invoice().min_final_cltv_expiry_delta() + 144;

    let request = AddHoldInvoiceRequest {
        hash: invoice_db.payment_hash().to_vec(),
        value_msat: amount_msats as i64,
        description_hash: desc_hash.to_vec(),
        expiry: 360,
        cltv_expiry,
        ..Default::default()
    };

    let client = &mut invoice_client.clone();

    let resp = client.add_hold_invoice(request).await?.into_inner();
    let inv = LnInvoice::from_str(&resp.payment_request)?;

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
) -> Result<Json<LnURLPayInvoice>, (StatusCode, String)> {
    match params.get("amount").and_then(|a| a.parse::<u64>().ok()) {
        None => Err((
            StatusCode::NOT_FOUND,
            String::from("{\"status\":\"ERROR\",\"reason\":\"Missing amount parameter.\"}"),
        )),
        Some(amount_msats) => {
            let zap_request = params.get("nostr").map(|event_str| {
                Event::from_json(event_str)
                    .map_err(|_| (StatusCode::NOT_FOUND, String::from("Invalid zap request")))
                    .unwrap()
            });

            let mut connection = state.db_pool.get().unwrap();

            let res = get_lnurl_invoice_impl(
                username,
                amount_msats,
                zap_request,
                &state.invoice_client,
                &mut connection,
            )
            .await;

            match res {
                Ok(Some(inv)) => {
                    let res = LnURLPayInvoice {
                        pr: inv.to_string(),
                    };
                    Ok(Json(res))
                }
                Ok(None) => Err((
                    StatusCode::NOT_FOUND,
                    String::from(
                        "{\"status\":\"ERROR\",\"reason\":\"Failed to generate invoice for user.\"}",
                    ),
                )),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "{{\"status\":\"ERROR\",\"reason\":\"Failed to generate invoice: {e}\"}}",
                    ),
                )),
            }
        }
    }
}