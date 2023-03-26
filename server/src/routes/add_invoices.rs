use crate::models::invoice::Invoice;
use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::secp256k1::SECP256K1;
use diesel::{RunQueryDsl, SqliteConnection};
pub use zap_tunnel_client::AddInvoices;

use crate::models::schema::*;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

pub(crate) fn add_invoices_impl(
    payload: AddInvoices,
    connection: &mut SqliteConnection,
) -> anyhow::Result<usize> {
    // validate signature
    payload.validate(SECP256K1)?;

    // get username
    let user = User::get_by_pubkey(connection, &payload.pubkey).ok_or(anyhow!("Invalid pubkey"))?;
    let username = user.username;

    let invoices: Vec<Invoice> = payload
        .invoices
        .iter()
        .map(|x| Invoice::new(x, Some(&username)))
        .collect();

    // insert invoices
    let num_inserted = diesel::insert_into(invoices::dsl::invoices)
        .values(&invoices)
        .execute(connection)?;

    println!("Added {} invoices for user {}", num_inserted, username);

    Ok(num_inserted)
}

pub async fn add_invoices(
    Extension(state): Extension<State>,
    Json(payload): Json<AddInvoices>,
) -> Result<Json<usize>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Failed to get database connection"),
        )
    })?;

    match add_invoices_impl(payload, &mut connection) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(handle_anyhow_error(e)),
    }
}
