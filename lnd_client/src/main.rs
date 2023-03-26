use std::str::FromStr;
use std::time::SystemTime;

use bitcoin::hashes::{sha256, Hash, HashEngine, Hmac, HmacEngine};
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use clap::Parser;
use lightning_invoice::Invoice as LnInvoice;
use tonic_openssl_lnd::lnrpc::{Invoice, SignMessageRequest};

use zap_tunnel_client::blocking::*;
use zap_tunnel_client::Builder;

use crate::config::*;

mod config;

const LUD_13_STRING: &str = "DO NOT EVER SIGN THIS TEXT WITH YOUR PRIVATE KEYS! IT IS ONLY USED FOR DERIVATION OF LNURL-AUTH HASHING-KEY, DISCLOSING ITS SIGNATURE WILL COMPROMISE YOUR LNURL-AUTH IDENTITY AND MAY LEAD TO LOSS OF FUNDS!";

#[tokio::main]
async fn main() {
    let config: Config = Config::parse();

    let proxy_host = config.proxy_host.clone();

    let client =
        BlockingClient::from_builder(Builder::new(&format!("http://{}", &proxy_host))).unwrap();

    let macaroon_file = config
        .macaroon_file
        .clone()
        .unwrap_or_else(|| default_macaroon_file(config.network.clone()));

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
    engine.input(proxy_host.as_bytes());
    let bytes = Hmac::<sha256::Hash>::from_engine(engine).into_inner();
    let key = SecretKey::from_slice(&bytes).unwrap();

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let context = Secp256k1::new();

    let invoices_remaining = match client.check_user(&context, now, &key) {
        Ok(check_user) => check_user.invoices_remaining,
        Err(_) => {
            let _ = client.create_user(&context, "test_user", &key).unwrap();
            0
        }
    };

    println!("Invoices remaining: {}", invoices_remaining);

    if invoices_remaining < 5 {
        let inv = Invoice {
            memo: "zap tunnel".to_string(),
            expiry: 31536000, // ~1 year
            private: true,
            ..Default::default()
        };
        let invoice = lnd_client
            .lightning()
            .clone()
            .add_invoice(inv)
            .await
            .unwrap()
            .into_inner();

        let ln_invoice = LnInvoice::from_str(&invoice.payment_request).unwrap();

        let num = client.add_invoices(&context, &key, &[ln_invoice]).unwrap();

        println!("Added {} invoices", num);
    }
}
