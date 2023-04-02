use std::str::FromStr;
use std::time::SystemTime;
use std::{thread, time};

use bitcoin::hashes::{sha256, Hash, HashEngine, Hmac, HmacEngine};
use bitcoin::secp256k1::{All, Secp256k1, SecretKey};
use clap::Parser;
use lightning_invoice::Invoice as LnInvoice;
use tonic_openssl_lnd::lnrpc::{Invoice, SignMessageRequest};
use tonic_openssl_lnd::LndLightningClient;

use zap_tunnel_client::blocking::*;
use zap_tunnel_client::Builder;

use crate::config::*;

mod config;

const LUD_13_STRING: &str = "DO NOT EVER SIGN THIS TEXT WITH YOUR PRIVATE KEYS! IT IS ONLY USED FOR DERIVATION OF LNURL-AUTH HASHING-KEY, DISCLOSING ITS SIGNATURE WILL COMPROMISE YOUR LNURL-AUTH IDENTITY AND MAY LEAD TO LOSS OF FUNDS!";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config: Config = Config::parse();

    let proxy_url = config.proxy_url.clone();

    let client = BlockingClient::from_builder(Builder::new(&String::from(proxy_url.clone())))?;

    let macaroon_file = config
        .macaroon_file
        .clone()
        .unwrap_or_else(|| default_macaroon_file(&config.network));

    let cert_file = config.cert_file.unwrap_or_else(default_cert_file);

    let mut lnd_client =
        tonic_openssl_lnd::connect(config.lnd_host, config.lnd_port, cert_file, macaroon_file)
            .await
            .expect("failed to connect");

    let req = SignMessageRequest {
        msg: LUD_13_STRING.as_bytes().to_vec(),
        ..Default::default()
    };
    let sig = lnd_client
        .lightning()
        .clone()
        .sign_message(req)
        .await
        .expect("failed to sign")
        .into_inner();

    let hashing_key = sha256::Hash::hash(&Vec::<u8>::from(sig.signature));

    let mut engine = HmacEngine::<sha256::Hash>::new(&hashing_key);
    let host = proxy_url.host().expect("failed to get host");
    engine.input(host.to_string().as_bytes());
    let bytes = Hmac::<sha256::Hash>::from_engine(engine).into_inner();
    let key = SecretKey::from_slice(&bytes)?;

    let context = Secp256k1::new();

    loop {
        check_status(
            &context,
            &key,
            config.invoice_cache,
            lnd_client.lightning().clone(),
            client.clone(),
        )
        .await?;
        thread::sleep(time::Duration::from_secs(60));
    }
}

async fn check_status(
    context: &Secp256k1<All>,
    key: &SecretKey,
    cache_size: usize,
    lnd_client: LndLightningClient,
    client: BlockingClient,
) -> anyhow::Result<usize> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    let invoices_remaining = match client.check_user(context, now, key) {
        Ok(check_user) => check_user.invoices_remaining,
        Err(_) => {
            // todo remove creating a test user and add actual create user flow
            let _ = client
                .create_user(context, "test_user", key)
                .expect("failed to create user");
            0
        }
    };

    println!("Invoices remaining: {}", invoices_remaining);

    let need_invoices = cache_size as i64 - invoices_remaining as i64;

    if need_invoices > 0 {
        let mut invoices: Vec<LnInvoice> = vec![];
        for _ in 0..need_invoices {
            let inv = Invoice {
                memo: "zap tunnel".to_string(),
                expiry: 31536000, // ~1 year
                private: true,
                ..Default::default()
            };
            let invoice = lnd_client.clone().add_invoice(inv).await?.into_inner();

            let ln_invoice = LnInvoice::from_str(&invoice.payment_request)?;

            invoices.push(ln_invoice);
        }

        let num = client.add_invoices(context, key, invoices.as_slice())?;
        println!("Added {} invoices", num);

        return Ok(num);
    }

    Ok(0)
}
