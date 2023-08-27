use std::str::FromStr;
use std::time::SystemTime;

use bitcoin::hashes::hex::ToHex;
use bitcoin::hashes::sha256::Hash as Sha256;
use diesel::prelude::*;
use lightning_invoice::Bolt11Invoice;

use super::schema::invoices;

#[derive(Queryable, AsChangeset, Insertable, Identifiable, Debug, Clone, PartialEq)]
#[diesel(primary_key(payment_hash))]
pub struct Invoice {
    payment_hash: String,
    pub invoice: String,
    pub expires_at: i64,
    pub wrapped_expiry: Option<i64>,
    fees_earned: Option<i64>,
    username: Option<String>,
}

pub const DEFAULT_INVOICE_EXPIRY: i64 = 360;

impl Invoice {
    pub fn new(invoice: &Bolt11Invoice, username: Option<&str>) -> Self {
        let expires_at: i64 = invoice
            .duration_since_epoch()
            .checked_add(invoice.expiry_time())
            .map_or(0, |t| t.as_secs() as i64);

        Self {
            payment_hash: invoice.payment_hash().to_hex(),
            invoice: invoice.to_string(),
            expires_at,
            wrapped_expiry: None,
            fees_earned: None,
            username: username.map(String::from),
        }
    }

    pub fn payment_hash(&self) -> Sha256 {
        Sha256::from_str(&self.payment_hash).expect("invalid payment hash")
    }

    pub fn invoice(&self) -> Bolt11Invoice {
        Bolt11Invoice::from_str(&self.invoice).expect("invalid invoice")
    }

    pub fn is_used(&self) -> bool {
        self.wrapped_expiry.is_some()
    }

    pub fn is_paid(&self) -> bool {
        self.fees_earned.is_some()
    }

    pub fn username(&self) -> Option<String> {
        self.username.clone()
    }

    pub fn set_wrapped_expiry(&mut self, wrapped_expiry: i64) {
        self.wrapped_expiry = Some(wrapped_expiry);
    }

    pub fn get_next_invoice(username: &str, conn: &mut SqliteConnection) -> anyhow::Result<Self> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        let w_expiry = now + DEFAULT_INVOICE_EXPIRY;

        conn.transaction(|conn| {
            let mut inv = invoices::table
                .filter(invoices::username.eq(username))
                .filter(invoices::fees_earned.is_null())
                .filter(invoices::wrapped_expiry.is_null())
                .filter(invoices::expires_at.gt(now))
                .order(invoices::expires_at.asc())
                .first::<Self>(conn)?;

            inv.set_wrapped_expiry(w_expiry);

            diesel::update(invoices::table)
                .filter(invoices::payment_hash.eq(&inv.payment_hash))
                .set(invoices::wrapped_expiry.eq(w_expiry))
                .execute(conn)?;

            Ok(inv)
        })
    }

    pub fn mark_invoice_paid(
        payment_hash: &str,
        fees_earned: i64,
        conn: &mut SqliteConnection,
    ) -> anyhow::Result<()> {
        diesel::update(invoices::table.filter(invoices::payment_hash.eq(&payment_hash)))
            .set(invoices::fees_earned.eq(Some(fees_earned)))
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
                    .and(invoices::fees_earned.is_null())
                    .and(invoices::wrapped_expiry.is_null())
                    .and(invoices::expires_at.gt(now)),
            )
            .first(conn)
            .expect("Error counting invoices");

        Ok(count)
    }

    pub fn get_active_invoices(conn: &mut SqliteConnection) -> anyhow::Result<Vec<Self>> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        let invoices = invoices::table
            .filter(invoices::fees_earned.is_null())
            .filter(invoices::wrapped_expiry.is_not_null())
            .filter(invoices::wrapped_expiry.gt(now))
            .load::<Self>(conn)?;

        Ok(invoices)
    }

    #[cfg(test)]
    pub fn update_expiry(&self, conn: &mut SqliteConnection) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        diesel::update(invoices::table)
            .filter(invoices::payment_hash.eq(&self.payment_hash))
            .set(invoices::expires_at.eq(now + 500))
            .execute(conn)?;

        Ok(())
    }
}
