use crate::models::schema::*;
use crate::models::user::User;
use crate::State;
use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::{Message, PublicKey, SECP256K1};
use diesel::{RunQueryDsl, SqliteConnection};
use serde::Deserialize;
use std::str::FromStr;

pub fn handle_anyhow_error(err: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, format!("{err}"))
}

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
    pubkey: String,
    signature: String,
}

impl CreateUser {
    fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    fn signature(&self) -> Result<bitcoin::secp256k1::ecdsa::Signature, String> {
        bitcoin::secp256k1::ecdsa::Signature::from_str(&self.signature).map_err(|e| e.to_string())
    }

    fn message_hash(username: &str) -> anyhow::Result<Message> {
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

fn create_user_impl(
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

#[cfg(test)]
mod test {
    use crate::router::CreateUser;
    use bitcoin::hashes::hex::ToHex;
    use bitcoin::secp256k1::rand::Rng;
    use bitcoin::secp256k1::{rand, PublicKey, SecretKey, SECP256K1};
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::MigrationHarness;

    fn gen_tmp_db_name() -> String {
        let rng = rand::thread_rng();
        let rand_string: String = rng
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(30)
            .collect::<Vec<u8>>()
            .to_hex();
        format!("/tmp/zap_tunnel_{}.sqlite", rand_string)
    }

    fn create_database(db_name: &str) -> SqliteConnection {
        let mut connection = SqliteConnection::establish(db_name).unwrap();

        connection
            .run_pending_migrations(crate::models::MIGRATIONS)
            .expect("migrations could not run");

        connection
    }

    fn teardown_database(db_name: &str) {
        std::fs::remove_file(db_name).unwrap();
    }

    #[test]
    fn test_create_user() {
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let username = String::from("test_user");
        let private_key = SecretKey::new(&mut rand::thread_rng());
        let pubkey = PublicKey::from_secret_key(&SECP256K1, &private_key);

        let signature =
            SECP256K1.sign_ecdsa_low_r(&CreateUser::message_hash(&username).unwrap(), &private_key);

        let payload = CreateUser {
            username: username.clone(),
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
        };

        let user = super::create_user_impl(payload, conn).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.auth_key(), pubkey);

        teardown_database(&db_name);
    }
}
