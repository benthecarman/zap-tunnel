use bitcoin::hashes::hex::ToHex;
use std::str::FromStr;

use bitcoin::hashes::sha256::Hash as Sha256;
use diesel::prelude::*;
use lightning_invoice::Invoice;
use nostr::prelude::Event;

use super::schema::zaps;

#[derive(Queryable, AsChangeset, Insertable, Debug, Clone, PartialEq)]
#[diesel(primary_key(payment_hash))]
pub struct Zap {
    payment_hash: String,
    invoice: String,
    pub request: String,
    note_id: Option<String>,
}

impl Zap {
    pub fn new(invoice: &Invoice, request: Event, note_id: Option<Sha256>) -> Self {
        Self {
            payment_hash: invoice.payment_hash().to_hex(),
            invoice: invoice.to_string(),
            request: request.as_json(),
            note_id: note_id.map(|hash| hash.to_hex()),
        }
    }

    pub fn payment_hash(&self) -> Sha256 {
        Sha256::from_str(&self.payment_hash).expect("invalid payment hash")
    }

    pub fn invoice(&self) -> Invoice {
        Invoice::from_str(&self.invoice).expect("invalid invoice")
    }

    pub fn zap_request(&self) -> Event {
        Event::from_json(&self.request).expect("invalid zap request")
    }

    pub fn note_id(&self) -> Option<Sha256> {
        self.note_id
            .as_ref()
            .map(|hash| Sha256::from_str(hash).expect("invalid note id"))
    }

    pub fn create(zap: Zap, conn: &mut SqliteConnection) -> Result<Self, diesel::result::Error> {
        diesel::insert_into(zaps::table)
            .values(zap)
            .get_result(conn)
    }
}
