use bitcoin::hashes::hex::ToHex;
use std::str::FromStr;

use bitcoin::hashes::sha256::Hash as Sha256;
use diesel::prelude::*;
use lightning_invoice::Invoice as LnInvoice;

use super::schema::invoices;

#[derive(Queryable, AsChangeset, Debug, Clone, PartialEq)]
#[diesel(primary_key(payment_hash))]
pub struct Invoice {
    payment_hash: String,
    invoice: String,
    pub expires_at: i64,
    paid: i32,
    username: String,
}

impl Invoice {
    pub fn new(invoice: LnInvoice, username: String) -> Self {
        let expires_at: i64 = invoice
            .duration_since_epoch()
            .checked_add(invoice.expiry_time())
            .map_or(0, |t| t.as_secs() as i64);

        Self {
            payment_hash: invoice.payment_hash().to_hex(),
            invoice: invoice.to_string(),
            expires_at,
            paid: 0,
            username,
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
}

#[derive(Insertable)]
#[diesel(table_name = invoices)]
pub struct NewInvoice<'a> {
    pub payment_hash: &'a str,
    pub invoice: &'a str,
    pub expires_at: i64,
    pub paid: i32,
    pub username: &'a str,
}
