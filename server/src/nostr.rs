use crate::models::schema::zaps::*;
use crate::models::zap::Zap;
use bitcoin::hashes::hex::ToHex;
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::rand::rngs::OsRng;
use bitcoin::secp256k1::rand::RngCore;
use bitcoin::secp256k1::SECP256K1;
use std::net::SocketAddr;

use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection};
use lightning::ln::PaymentSecret;
use lightning_invoice::{Currency, InvoiceBuilder};
use nostr::key::SecretKey;
use nostr::prelude::ToBech32;
use nostr::{EventBuilder, Keys};
use nostr_sdk::Client;

const RELAYS: [&str; 8] = [
    "wss://nostr.mutinywallet.com",
    "wss://relay.snort.social",
    "wss://relay.nostr.band",
    "wss://eden.nostr.land",
    "wss://nos.lol",
    "wss://nostr.fmt.wiz.biz",
    "wss://relay.damus.io",
    "wss://nostr.wine",
];

pub async fn handle_zap(
    invoice_hash: &Vec<u8>,
    nostr_keys: &Keys,
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

        let fake_invoice = InvoiceBuilder::new(Currency::Bitcoin)
            .amount_milli_satoshis(amt_msats)
            .invoice_description(zap_invoice.description())
            .current_timestamp()
            .payment_hash(invoice_hash)
            .payment_secret(PaymentSecret(*payment_secret))
            .min_final_cltv_expiry_delta(144)
            .build_signed(|hash| SECP256K1.sign_ecdsa_recoverable(hash, &private_key))?;

        let event = EventBuilder::new_zap_receipt(
            fake_invoice.to_string(),
            Some(preimage.to_hex()),
            zap_request,
        )
        .to_event(nostr_keys)?;

        // Create new client
        let client = Client::new(nostr_keys);
        let relays: Vec<(String, Option<SocketAddr>)> =
            RELAYS.into_iter().map(|r| (r.to_string(), None)).collect();
        client.add_relays(relays).await?;
        client.connect().await;

        let event_id = client.send_event(event).await?;

        client.disconnect().await?;

        println!(
            "Broadcasted event id: {}!",
            event_id.to_bech32().expect("bech32")
        );

        // update zap db
        diesel::update(dsl::zaps.find(invoice_hash.to_hex()))
            .set(note_id.eq(event_id.to_hex()))
            .execute(db)?;

        Ok(())
    } else {
        Ok(())
    }
}
