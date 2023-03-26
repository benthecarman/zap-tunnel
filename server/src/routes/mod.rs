use axum::http::StatusCode;
use axum::response::Html;
use axum::Extension;
use dioxus::prelude::*;

pub use create_user::create_user;

use crate::State;

mod create_user;

pub(crate) fn handle_anyhow_error(err: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, format!("{err}"))
}

pub async fn index(Extension(state): Extension<State>) -> Html<String> {
    let connect = format!("Connect with me here: {}", state.connection_string);

    Html(dioxus::ssr::render_lazy(rsx! {
            h1 { "Hello world!" }
            p {"{connect}"}
    }))
}

#[cfg(test)]
mod test {
    use bitcoin::hashes::hex::ToHex;
    use bitcoin::secp256k1::rand::Rng;
    use bitcoin::secp256k1::{rand, PublicKey, SecretKey, SECP256K1};
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::MigrationHarness;

    use crate::routes::create_user::CreateUser;

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

        let user = super::create_user::create_user_impl(payload, conn).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.auth_key(), pubkey);

        teardown_database(&db_name);
    }
}
