use diesel_migrations::{embed_migrations, EmbeddedMigrations};

pub mod invoice;
pub mod schema;
pub mod user;
pub mod zap;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

#[cfg(test)]
mod test {
    use crate::models::invoice::*;
    use crate::models::user::*;
    use crate::models::zap::*;
    use bitcoin::hashes::hex::ToHex;
    use bitcoin::secp256k1::rand::Rng;
    use bitcoin::secp256k1::{rand, PublicKey};
    use diesel::associations::HasTable;
    use diesel::{Connection, QueryDsl, RunQueryDsl, SqliteConnection, SqliteExpressionMethods};
    use diesel_migrations::MigrationHarness;
    use lightning_invoice::Invoice as LnInvoice;
    use std::str::FromStr;

    const PUB_KEY_STR: &str = "032e58afe51f9ed8ad3cc7897f634d881fdbe49a81564629ded8156bebd2ffd1af";
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
    fn test_create_and_find_user() {
        use super::schema::users::dsl::*;
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let new_user = User::new("test_user", PublicKey::from_str(PUB_KEY_STR).unwrap());

        // create user
        let size = diesel::insert_into(users::table())
            .values(&new_user)
            .execute(conn)
            .unwrap();

        assert_eq!(size, 1);

        // get user
        let user = users
            .filter(username.is("test_user"))
            .first::<User>(conn)
            .unwrap();

        assert_eq!(user.username, "test_user");
        assert_eq!(user.pubkey().to_string(), PUB_KEY_STR);

        teardown_database(&db_name);
    }

    #[test]
    fn test_create_and_find_invoice() {
        use super::schema::invoices::dsl::*;
        use super::schema::users::dsl::*;
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let test_username: String = String::from("test_user");

        let new_user = User::new(&test_username, PublicKey::from_str(PUB_KEY_STR).unwrap());
        // create user
        let size = diesel::insert_into(users::table())
            .values(&new_user)
            .execute(conn)
            .unwrap();
        assert_eq!(size, 1);

        let inv: LnInvoice = LnInvoice::from_str(INVOICE_STR).unwrap();

        let expiry: i64 = 1631312603;

        let new_invoice = Invoice::new(&inv, &test_username);

        // create invoice
        let size = diesel::insert_into(invoices::table())
            .values(&new_invoice)
            .execute(conn)
            .unwrap();
        assert_eq!(size, 1);

        // get invoice
        let invoice_db = invoices
            .filter(payment_hash.is(inv.payment_hash().to_hex()))
            .first::<Invoice>(conn)
            .unwrap();

        assert_eq!(invoice_db.payment_hash(), inv.payment_hash().clone());
        assert_eq!(invoice_db.invoice().to_string(), INVOICE_STR);
        assert_eq!(invoice_db.is_paid(), false);
        assert_eq!(invoice_db.expires_at, expiry);
        assert_eq!(invoice_db.username(), test_username);

        teardown_database(&db_name);
    }

    #[test]
    fn test_create_and_find_zap() {
        use super::schema::zaps::dsl::*;
        let db_name = gen_tmp_db_name();
        let conn = &mut create_database(&db_name);

        let inv: LnInvoice = LnInvoice::from_str(INVOICE_STR).unwrap();
        let zap_request = nostr::Event::from_json("{\"pubkey\":\"32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245\",\"content\":\"\",\"id\":\"d9cc14d50fcb8c27539aacf776882942c1a11ea4472f8cdec1dea82fab66279d\",\"created_at\":1674164539,\"sig\":\"77127f636577e9029276be060332ea565deaf89ff215a494ccff16ae3f757065e2bc59b2e8c113dd407917a010b3abd36c8d7ad84c0e3ab7dab3a0b0caa9835d\",\"kind\":9734,\"tags\":[[\"e\",\"3624762a1274dd9636e0c552b53086d70bc88c165bc4dc0f9e836a1eaf86c3b8\"],[\"p\",\"32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245\"],[\"relays\",\"wss://relay.damus.io\",\"wss://nostr-relay.wlvs.space\",\"wss://nostr.fmt.wiz.biz\",\"wss://relay.nostr.bg\",\"wss://nostr.oxtr.dev\",\"wss://nostr.v0l.io\",\"wss://brb.io\",\"wss://nostr.bitcoiner.social\",\"ws://monad.jb55.com:8080\",\"wss://relay.snort.social\"]]}").unwrap();

        let new_zap = Zap::new(&inv, zap_request, None);

        // create zap
        let size = diesel::insert_into(zaps::table())
            .values(&new_zap)
            .execute(conn)
            .unwrap();
        assert_eq!(size, 1);

        // get zap
        let zap = zaps
            .filter(payment_hash.is(inv.payment_hash().to_hex()))
            .first::<Zap>(conn)
            .unwrap();

        assert_eq!(zap.payment_hash(), inv.payment_hash().clone());
        assert_eq!(zap.invoice().to_string(), INVOICE_STR);
        assert_eq!(
            zap.zap_request().id.to_hex(),
            "d9cc14d50fcb8c27539aacf776882942c1a11ea4472f8cdec1dea82fab66279d"
        );
        assert_eq!(zap.note_id(), None);

        teardown_database(&db_name);
    }
}
