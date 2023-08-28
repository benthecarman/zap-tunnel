//! LNURL by way of `reqwest` HTTP client.
#![allow(clippy::result_large_err)]

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey, Signing};
use lightning_invoice::Bolt11Invoice;
use reqwest::Client;

use crate::api::*;
use crate::{Builder, Error};

#[derive(Debug)]
pub struct AsyncClient {
    pub url: String,
    client: Client,
}

impl AsyncClient {
    /// build an async client from a builder
    pub fn from_builder(builder: Builder) -> Result<Self, Error> {
        let mut client_builder = Client::builder();

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(proxy) = &builder.proxy {
            client_builder = client_builder.proxy(reqwest::Proxy::all(proxy)?);
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(timeout) = builder.timeout {
            client_builder = client_builder.timeout(core::time::Duration::from_secs(timeout));
        }

        Ok(Self::from_client(builder.base_url, client_builder.build()?))
    }

    /// build an async client from the base url and [`Client`]
    pub fn from_client(url: String, client: Client) -> Self {
        AsyncClient { url, client }
    }

    pub async fn create_user<C: Signing>(
        &self,
        context: &Secp256k1<C>,
        username: &str,
        private_key: &SecretKey,
    ) -> Result<CreateUserResponse, Error> {
        let pubkey = PublicKey::from_secret_key(context, private_key);

        let signature = context.sign_ecdsa_low_r(
            &CreateUser::message_hash(username).expect("Failed to create hash"),
            private_key,
        );

        let payload = CreateUser {
            username: String::from(username),
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
        };

        let resp = self
            .client
            .post(&format!("{}/create-user", self.url))
            .body(serde_json::to_vec(&payload)?)
            .send()
            .await?;

        Ok(resp.error_for_status()?.json().await?)
    }

    pub async fn check_user<C: Signing>(
        &self,
        context: &Secp256k1<C>,
        current_time: u64,
        private_key: &SecretKey,
    ) -> Result<CheckUser, Error> {
        let pubkey = PublicKey::from_secret_key(context, private_key);

        let signature = context.sign_ecdsa_low_r(
            &CheckUser::message_hash(current_time).expect("Failed to create hash"),
            private_key,
        );

        let resp = self
            .client
            .get(&format!(
                "{}/check-user?time={}&pubkey={}&signature={}",
                self.url, current_time, pubkey, signature
            ))
            .send()
            .await?;

        Ok(resp.error_for_status()?.json().await?)
    }

    pub async fn add_invoices<C: Signing>(
        &self,
        context: &Secp256k1<C>,
        private_key: &SecretKey,
        invoices: &[Bolt11Invoice],
    ) -> Result<usize, Error> {
        let pubkey = PublicKey::from_secret_key(context, private_key);

        let signature = context.sign_ecdsa_low_r(
            &AddInvoices::message_hash(invoices).expect("Failed to create hash"),
            private_key,
        );

        let payload = AddInvoices {
            pubkey: pubkey.to_string(),
            signature: signature.to_string(),
            invoices: invoices.to_vec(),
        };

        let resp = self
            .client
            .post(&format!("{}/add-invoices", self.url))
            .body(serde_json::to_vec(&payload)?)
            .send()
            .await?;

        Ok(resp.error_for_status()?.json().await?)
    }
}
