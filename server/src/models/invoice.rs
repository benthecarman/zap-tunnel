use std::str::FromStr;
use std::time::SystemTime;

use bitcoin::hashes::hex::ToHex;
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
    used: i32,
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
            used: 0,
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

    pub fn is_used(&self) -> bool {
        self.used != 0
    }

    pub fn is_paid(&self) -> bool {
        self.paid != 0
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn set_paid(&mut self) {
        self.paid = 1;
    }

    pub fn get_next_invoice(username: String, conn: &mut SqliteConnection) -> anyhow::Result<Self> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        conn.transaction(|conn| {
            let invoice = invoices::table
                .filter(invoices::username.eq(username))
                .filter(invoices::paid.eq(0))
                .filter(invoices::used.eq(0))
                .filter(invoices::expires_at.gt(now))
                .order(invoices::expires_at.asc())
                .first::<Self>(conn)?;

            diesel::update(
                invoices::table.filter(invoices::payment_hash.eq(&invoice.payment_hash)),
            )
            .set(invoices::used.eq(1))
            .execute(conn)?;

            Ok(invoice)
        })
    }

    pub fn mark_invoice_paid(
        payment_hash: &str,
        conn: &mut SqliteConnection,
    ) -> anyhow::Result<()> {
        diesel::update(invoices::table.filter(invoices::payment_hash.eq(&payment_hash)))
            .set(invoices::paid.eq(1))
            .execute(conn)?;

        Ok(())
    }

    pub fn get_num_invoices_available(
        username: &str,
        conn: &mut SqliteConnection,
    ) -> anyhow::Result<i64> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        let count: i64 = invoices::table
            .select(diesel::dsl::count_star())
            .filter(
                invoices::username
                    .eq(username)
                    .and(invoices::paid.eq(0))
                    .and(invoices::used.eq(0))
                    .and(invoices::expires_at.gt(now)),
            )
            .first(conn)
            .expect("Error counting invoices");

        Ok(count)
    }
}
