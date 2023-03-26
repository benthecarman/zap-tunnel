use std::str::FromStr;

use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::{Message, PublicKey, SECP256K1};
use diesel::{RunQueryDsl, SqliteConnection};
use serde::Deserialize;

use crate::models::schema::*;
use crate::models::user::User;
use crate::routes::handle_anyhow_error;
use crate::State;

#[derive(Deserialize)]
pub struct CreateUser {
    pub(crate) username: String,
    pub(crate) pubkey: String,
    pub(crate) signature: String,
}

impl CreateUser {
    fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    fn signature(&self) -> Result<bitcoin::secp256k1::ecdsa::Signature, String> {
        bitcoin::secp256k1::ecdsa::Signature::from_str(&self.signature).map_err(|e| e.to_string())
    }

    pub(crate) fn message_hash(username: &str) -> anyhow::Result<Message> {
        let hash = Sha256::hash(username.as_bytes());

        Ok(Message::from_slice(&hash)?)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.username.len() < 3 {
            return Err(anyhow!("Username must be at least 3 characters long"));
        }

        let pubkey = self.pubkey().map_err(|_| anyhow!("Invalid pubkey"))?;
        let signature = self.signature().map_err(|_| anyhow!("Invalid signature"))?;

        let msg = CreateUser::message_hash(&self.username)?;

        if SECP256K1.verify_ecdsa(&msg, &signature, &pubkey).is_err() {
            return Err(anyhow!("Invalid signature"));
        }

        Ok(())
    }
}

pub(crate) fn create_user_impl(
    payload: CreateUser,
    connection: &mut SqliteConnection,
) -> anyhow::Result<User> {
    // validate username and signature
    payload.validate()?;

    let new_user = User::new(payload.username.clone(), payload.pubkey()?);

    // create user
    let user: User = diesel::insert_into(users::dsl::users)
        .values(&new_user)
        .get_result(connection)
        .unwrap();

    Ok(user)
}

pub async fn create_user(
    Extension(state): Extension<State>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<User>, (StatusCode, String)> {
    let mut connection = state.db_pool.get().unwrap();

    match create_user_impl(payload, &mut connection) {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(handle_anyhow_error(e)),
    }
}
