use std::str::FromStr;

use crate::models::invoice::Invoice;
use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::{Message, PublicKey, SECP256K1};
use diesel::{RunQueryDsl, SqliteConnection};
use lightning_invoice::Invoice as LnInvoice;
use serde::Deserialize;

use crate::models::schema::*;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

#[derive(Deserialize, Debug, Clone)]
pub struct AddInvoices {
    pub(crate) pubkey: String,
    pub(crate) signature: String,
    pub(crate) invoices: Vec<LnInvoice>,
}

impl AddInvoices {
    fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    fn signature(&self) -> Result<bitcoin::secp256k1::ecdsa::Signature, String> {
        bitcoin::secp256k1::ecdsa::Signature::from_str(&self.signature).map_err(|e| e.to_string())
    }

    pub(crate) fn message_hash(invoices: &[LnInvoice]) -> anyhow::Result<Message> {
        let bytes: Vec<u8> = invoices.iter().fold(Vec::new(), |mut acc, x| {
            acc.extend(x.payment_hash().to_vec());
            acc
        });
        let hash = Sha256::hash(&bytes);

        Ok(Message::from_slice(&hash)?)
    }

    fn validate(&self) -> anyhow::Result<()> {
        let pubkey = self.pubkey().map_err(|_| anyhow!("Invalid pubkey"))?;
        let signature = self.signature().map_err(|_| anyhow!("Invalid signature"))?;

        let message_hash = Self::message_hash(&self.invoices)?;

        if SECP256K1
            .verify_ecdsa(&message_hash, &signature, &pubkey)
            .is_err()
        {
            return Err(anyhow!("Invalid signature"));
        }

        Ok(())
    }
}

pub(crate) fn add_invoices_impl(
    payload: AddInvoices,
    connection: &mut SqliteConnection,
) -> anyhow::Result<usize> {
    // validate signature
    payload.validate()?;

    // get username
    let user =
        User::get_by_auth_key(connection, &payload.pubkey).ok_or(anyhow!("Invalid pubkey"))?;
    let username = user.username;

    let invoices: Vec<Invoice> = payload
        .invoices
        .iter()
        .map(|x| Invoice::new(x, &username))
        .collect();

    // insert invoices
    let num_inserted = diesel::insert_into(invoices::dsl::invoices)
        .values(&invoices)
        .execute(connection)?;

    Ok(num_inserted)
}

pub async fn add_invoices(
    Extension(state): Extension<State>,
    Json(payload): Json<AddInvoices>,
) -> Result<Json<usize>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().unwrap();

    match add_invoices_impl(payload, &mut connection) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(handle_anyhow_error(e)),
    }
}
