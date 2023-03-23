use crate::models::schema::zaps::*;
use crate::models::zap::Zap;
use bitcoin::hashes::hex::ToHex;
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::rand::rngs::OsRng;
use bitcoin::secp256k1::rand::RngCore;
use std::net::SocketAddr;

use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection};
use lightning::ln::PaymentSecret;
use lightning_invoice::{Currency, Invoice, InvoiceBuilder, InvoiceDescription};
use nostr::key::SecretKey;
use nostr::prelude::TagKind::Custom;
use nostr::Tag::Generic;
use nostr::{EventBuilder, Keys, Kind};
use nostr_sdk::Client;

const RELAYS: [&str; 9] = [
    "wss://nostr.mutinywallet.com",
    "wss://nostr.zebedee.cloud",
    "wss://relay.snort.social",
    "wss://relay.nostr.band",
    "wss://eden.nostr.land",
    "wss://nos.lol",
    "wss://nostr.fmt.wiz.biz",
    "wss://relay.damus.io",
    "wss://nostr.wine",
];

pub async fn handle_zap(
    invoice_hash: Vec<u8>,
    nostr_keys: Keys,
    db: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
) -> anyhow::Result<()> {
    let zap_opt: Option<Zap> = dsl::zaps
        .filter(payment_hash.eq(invoice_hash.to_hex()))
        .filter(note_id.is_null())
        .first::<Zap>(db)
        .optional()?;

    if let Some(zap) = zap_opt {
        let zap_request = zap.zap_request();
        let zap_invoice = zap.invoice();
        let desc_hash = match zap_invoice.description() {
            InvoiceDescription::Direct(_) => return Err(anyhow::anyhow!("direct description")),
            InvoiceDescription::Hash(hash) => hash.0,
        };

        let preimage = &mut [0u8; 32];
        OsRng.fill_bytes(preimage);
        let invoice_hash = Sha256::hash(preimage);

        let payment_secret = &mut [0u8; 32];
        OsRng.fill_bytes(payment_secret);

        let priv_key_bytes = &mut [0u8; 32];
        OsRng.fill_bytes(priv_key_bytes);
        let private_key = SecretKey::from_slice(priv_key_bytes)?;

        let amt_msats = zap_invoice
            .amount_milli_satoshis()
            .expect("Invoice must have an amount");

        let raw_invoice = InvoiceBuilder::new(Currency::Bitcoin)
            .amount_milli_satoshis(amt_msats)
            .description_hash(desc_hash)
            .current_timestamp()
            .payment_hash(invoice_hash)
            .payment_secret(PaymentSecret(*payment_secret))
            .min_final_cltv_expiry_delta(144)
            .build_raw()
            .and_then(|raw| {
                raw.sign(|hash| {
                    Ok(bitcoin::secp256k1::Secp256k1::new()
                        .sign_ecdsa_recoverable(hash, &private_key))
                })
            })?;

        let fake_invoice = Invoice::from_signed(raw_invoice)?;

        let invoice_tag = Generic(Custom("bolt11".to_string()), vec![fake_invoice.to_string()]);
        let preimage_tag = Generic(Custom("preimage".to_string()), vec![preimage.to_hex()]);
        let description_tag = Generic(Custom("description".to_string()), vec![zap.request]);
        let amount_tag = Generic(Custom("amount".to_string()), vec![amt_msats.to_string()]);

        let mut tags = vec![invoice_tag, preimage_tag, description_tag, amount_tag];

        // add e tag
        if let Some(tag) = zap_request
            .tags
            .clone()
            .into_iter()
            .find(|t| t.as_vec().first() == Some(&"e".to_string()))
        {
            tags.push(tag);
        }

        // add p tag
        if let Some(tag) = zap_request
            .tags
            .into_iter()
            .find(|t| t.as_vec().first() == Some(&"p".to_string()))
        {
            tags.push(tag);
        }

        let event = EventBuilder::new(Kind::Zap, "", &tags).to_event(&nostr_keys)?;

        // Create new client
        let client = Client::new(&nostr_keys);
        let relays: Vec<(String, Option<SocketAddr>)> =
            RELAYS.into_iter().map(|r| (r.to_string(), None)).collect();
        client.add_relays(relays).await?;

        let event_id = client.send_event(event).await?;

        println!("Broadcasted event id: {}!", event_id);

        // update zap db
        diesel::update(dsl::zaps.find(invoice_hash.to_hex()))
            .set(note_id.eq(event_id.to_hex()))
            .execute(db)?;

        Ok(())
    } else {
        Ok(())
    }
}
