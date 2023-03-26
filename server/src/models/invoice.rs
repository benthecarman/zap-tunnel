use bitcoin::hashes::hex::ToHex;
use std::str::FromStr;
use std::time::SystemTime;

use bitcoin::hashes::sha256::Hash as Sha256;
use diesel::prelude::*;
use lightning_invoice::Invoice as LnInvoice;

use super::schema::invoices;

#[derive(Queryable, AsChangeset, Insertable, Debug, Clone, PartialEq)]
#[diesel(primary_key(payment_hash))]
pub struct Invoice {
    payment_hash: String,
    invoice: String,
    pub expires_at: i64,
    paid: i32,
    username: String,
}

impl Invoice {
    pub fn new(invoice: &LnInvoice, username: &str) -> Self {
        let expires_at: i64 = invoice
            .duration_since_epoch()
            .checked_add(invoice.expiry_time())
            .map_or(0, |t| t.as_secs() as i64);

        Self {
            payment_hash: invoice.payment_hash().to_hex(),
            invoice: invoice.to_string(),
            expires_at,
            paid: 0,
            username: String::from(username),
        }
    }

    pub fn payment_hash(&self) -> Sha256 {
        Sha256::from_str(&self.payment_hash).expect("invalid payment hash")
    }

    pub fn invoice(&self) -> LnInvoice {
        LnInvoice::from_str(&self.invoice).expect("invalid invoice")
    }

    pub fn is_paid(&self) -> bool {
        self.paid == 1
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn set_paid(&mut self) {
        self.paid = 1;
    }

    // todo mark invoice as used
    pub fn get_next_invoice(username: String, conn: &mut SqliteConnection) -> Option<Self> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        invoices::table
            .filter(invoices::username.eq(username))
            .filter(invoices::paid.eq(0))
            .filter(invoices::expires_at.gt(now))
            .order(invoices::expires_at.asc())
            .first::<Self>(conn)
            .ok()
    }

    pub fn get_num_invoices_available(
        username: &str,
        conn: &mut SqliteConnection,
    ) -> anyhow::Result<i64> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Ok(invoices::table
            .filter(invoices::username.eq(username))
            .filter(invoices::paid.eq(0))
            .filter(invoices::expires_at.gt(now))
            .count()
            .get_result(conn)?)
    }
}
