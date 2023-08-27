use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::secp256k1::SECP256K1;
use bitcoin::Network;
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use lightning_invoice::Bolt11Invoice;
use lightning_invoice::Currency;

pub use zap_tunnel_client::AddInvoices;

use crate::models::invoice::Invoice;
use crate::models::schema::*;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

fn check_invoices(invoices: &[Bolt11Invoice], network: Network) -> bool {
    let expected_currency = match network {
        Network::Bitcoin => Currency::Bitcoin,
        Network::Testnet => Currency::BitcoinTestnet,
        Network::Signet => Currency::Signet,
        Network::Regtest => Currency::Regtest,
    };

    invoices.iter().all(|inv| {
        inv.currency() == expected_currency
            && inv.amount_milli_satoshis().is_none()
            && !inv.is_expired()
            && inv.min_final_cltv_expiry_delta() < 333
            && inv.check_signature().is_ok()
    })
}

pub(crate) fn add_invoices_impl(
    payload: AddInvoices,
    connection: &mut SqliteConnection,
) -> anyhow::Result<usize> {
    // validate signature
    payload.validate(SECP256K1)?;

    connection.transaction(|connection| {
        // get username
        let user =
            User::get_by_pubkey(connection, &payload.pubkey()?).ok_or(anyhow!("Invalid pubkey"))?;
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
    })
}

pub async fn add_invoices(
    Extension(state): Extension<State>,
    Json(payload): Json<AddInvoices>,
) -> Result<Json<usize>, (StatusCode, String)> {
    if payload.invoices.is_empty() || !check_invoices(&payload.invoices, state.config.network) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid invoices, must have no amount and be for network: {}",
                state.config.network
            ),
        ));
    }

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
