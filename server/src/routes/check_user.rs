use std::collections::HashMap;
use std::str::FromStr;

use anyhow::anyhow;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::secp256k1::ecdsa::Signature;
use bitcoin::secp256k1::{PublicKey, SECP256K1};
use diesel::SqliteConnection;

pub use zap_tunnel_client::CheckUser;

use crate::models::invoice::Invoice;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

pub(crate) fn check_user_impl(
    time: u64,
    pubkey: &PublicKey,
    signature: &Signature,
    connection: &mut SqliteConnection,
) -> anyhow::Result<CheckUser> {
    // validate username and signature
    CheckUser::validate(SECP256K1, time, pubkey, signature)?;

    let user = User::get_by_pubkey(connection, &pubkey.to_string())
        .ok_or(anyhow!("No user found with pubkey {}", pubkey.to_string()))?;
    let num_invoices: i64 = Invoice::get_num_invoices_available(&user.username, connection)?;

    Ok(CheckUser {
        username: user.username,
        pubkey: pubkey.to_string(),
        invoices_remaining: num_invoices as u64,
    })
}

pub async fn check_user(
    Extension(state): Extension<State>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<CheckUser>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database connection"),
        )
    })?;

    let time = params.get("time").and_then(|p| p.parse::<u64>().ok());
    let pubkey = params
        .get("pubkey")
        .and_then(|p| PublicKey::from_str(p).ok());
    let signature = params
        .get("signature")
        .and_then(|p| Signature::from_str(p).ok());

    if time.is_none() || pubkey.is_none() || signature.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            String::from("Missing required parameters"),
        ));
    }

    match check_user_impl(
        time.expect("Checked above"),
        &pubkey.expect("Checked above"),
        &signature.expect("Checked above"),
        &mut connection,
    ) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(handle_anyhow_error(e)),
    }
}
