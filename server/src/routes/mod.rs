use axum::http::StatusCode;
use axum::response::Html;
use axum::Extension;
use dioxus::prelude::*;

pub use add_invoices::add_invoices;
pub use check_user::check_user;
pub use create_user::create_user;
pub use lnurlp::{get_lnurl_invoice, get_lnurlp};

use crate::State;

mod add_invoices;
mod check_user;
mod create_user;
mod lnurlp;

pub(crate) fn handle_anyhow_error(err: anyhow::Error) -> (StatusCode, String) {
    println!("Error: {err}");
    (StatusCode::BAD_REQUEST, err.to_string())
}

pub async fn index(Extension(state): Extension<State>) -> Html<String> {
    let connect = format!(
        "This Zap Tunnel is currently running on the following node: {}",
        state.connection_string
    );

    Html(dioxus_ssr::render_lazy(rsx! {
        style { include_str!("../style.css") }
        head { title { "Zap Tunnel" } }
        h1 { "Welcome to my Zap Tunnel!" }
        p { "This allows you to receive zaps to a self-custodial wallet without having to setup a web server!" }
        p { "Zap Tunnel works similar to lnproxy but with slightly different trust assumptions." }
        p { "A Zap Tunnel stores a bunch of amount-less invoices on behalf of you and serves wrapped versions of them when an invoice is requested. \
        Because of this you are trusting that the Zap Tunnel will not just give out its own invoices, and that it won't siphon portions of the funds routed through it." }
        p { "Because this is meant to be a replacement for a custodial lightning address, it should be an okay trust assumption" }
        br {}
        p {"{connect}"}
    }))
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use bitcoin::hashes::hex::ToHex;
    use bitcoin::secp256k1::rand::Rng;
    use bitcoin::secp256k1::{rand, PublicKey, SecretKey, SECP256K1};
    use diesel::{Connection, SqliteConnection};
    use diesel_migrations::MigrationHarness;
    use lightning_invoice::Bolt11Invoice;
    use lnurl::Tag;

    use crate::routes::add_invoices::AddInvoices;
    use crate::routes::create_user::CreateUser;

    const INVOICE_STR: &str = "lnbc30110n1psnhkd0pp5pa3778sup4c5h6adqjxcygwejqhrczfuverex9meta4amp7jpfdqdz8fag975j92324yn3qgfhhgw3qwa58jgryd9jzq7t0w5sxgetrdajx2grd0ysxjmnkda5kxegcqzpgxqzfvsp5uejqpus5df8tyf5kmfxpkq6r80up4r9ahewtl8qz6a9enn7e0ums9qyyssqyf8m5yy8y4s4shnr9psx0lm27h94dg2j9wqd6nanrymhnztdwaujk854vw98500vmleeymsywysltdaymlmxp2fr6t49f69a6xfd9tspy50l7d";

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
        let pubkey = PublicKey::from_secret_key(SECP256K1, &private_key);

        let signature =
            SECP256K1.sign_ecdsa_low_r(&CreateUser::message_hash(&username).unwrap(), &private_key);

        let payload = CreateUser {
            username: username.clone(),
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
        };

        let user = super::create_user::create_user_impl(payload, conn).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.pubkey(), pubkey);

        teardown_database(&db_name);
    }

    #[test]
    fn test_lnurlp() {
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let username = String::from("test_user");
        let private_key = SecretKey::new(&mut rand::thread_rng());
        let pubkey = PublicKey::from_secret_key(SECP256K1, &private_key);

        let signature =
            SECP256K1.sign_ecdsa_low_r(&CreateUser::message_hash(&username).unwrap(), &private_key);

        let payload = CreateUser {
            username: username.clone(),
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
        };

        let user = super::create_user::create_user_impl(payload, conn).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.pubkey(), pubkey);

        let config = crate::config::Config::dummy();

        let lnurlp = super::lnurlp::get_lnurlp_impl(user.username, &config, conn).unwrap();

        assert_eq!(lnurlp.allows_nostr, Some(true));
        assert!(lnurlp.callback.len() > 1);
        assert_eq!(lnurlp.tag, Tag::PayRequest);

        teardown_database(&db_name);
    }

    #[test]
    fn test_add_invoice() {
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let username = String::from("test_user");
        let private_key = SecretKey::new(&mut rand::thread_rng());
        let pubkey = PublicKey::from_secret_key(SECP256K1, &private_key);

        let signature =
            SECP256K1.sign_ecdsa_low_r(&CreateUser::message_hash(&username).unwrap(), &private_key);

        let payload = CreateUser {
            username: username.clone(),
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
        };

        let user = super::create_user::create_user_impl(payload, conn).unwrap();

        assert_eq!(user.username, username);
        assert_eq!(user.pubkey(), pubkey);

        let ln_invoice = Bolt11Invoice::from_str(INVOICE_STR).unwrap();

        let signature = SECP256K1.sign_ecdsa_low_r(
            &AddInvoices::message_hash(&[ln_invoice.clone()]).unwrap(),
            &private_key,
        );

        let payload = AddInvoices {
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
            invoices: vec![ln_invoice],
        };

        let num_added = super::add_invoices::add_invoices_impl(payload, conn).unwrap();

        assert_eq!(num_added, 1);

        teardown_database(&db_name);
    }
}
