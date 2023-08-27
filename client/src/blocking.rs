//! LNURL by way of `ureq` HTTP client.
#![allow(clippy::result_large_err)]

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey, Signing};
use lightning_invoice::Bolt11Invoice;
use std::time::Duration;

use ureq::{Agent, Proxy};

use crate::{AddInvoices, Builder, CheckUser, CreateUser, CreateUserResponse, Error};

#[derive(Debug, Clone)]
pub struct BlockingClient {
    pub url: String,
    agent: Agent,
}

impl BlockingClient {
    /// build a blocking client from a [`Builder`]
    pub fn from_builder(builder: Builder) -> Result<Self, Error> {
        let mut agent_builder = ureq::AgentBuilder::new();

        if let Some(timeout) = builder.timeout {
            agent_builder = agent_builder.timeout(Duration::from_secs(timeout));
        }

        if let Some(proxy) = &builder.proxy {
            agent_builder = agent_builder.proxy(Proxy::new(proxy).expect("Failed to create proxy"));
        }

        Ok(Self::from_agent(builder.base_url, agent_builder.build()))
    }

    /// build a blocking client from an [`Agent`]
    pub fn from_agent(url: String, agent: Agent) -> Self {
        BlockingClient { url, agent }
    }

    pub fn create_user<C: Signing>(
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
            .agent
            .post(&format!("{}/create-user", self.url))
            .send_json(payload);

        match resp {
            Ok(resp) => Ok(resp.into_json()?),
            Err(ureq::Error::Status(code, resp)) => {
                let str = resp.into_string().ok();
                Err(Error::HttpResponse(code, str))
            }
            Err(e) => Err(Error::Ureq(e)),
        }
    }

    pub fn check_user<C: Signing>(
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
            .agent
            .get(&format!(
                "{}/check-user?time={}&pubkey={}&signature={}",
                self.url, current_time, pubkey, signature
            ))
            .call();

        match resp {
            Ok(resp) => Ok(resp.into_json()?),
            Err(ureq::Error::Status(code, resp)) => {
                let str = resp.into_string().ok();
                Err(Error::HttpResponse(code, str))
            }
            Err(e) => Err(Error::Ureq(e)),
        }
    }

    pub fn add_invoices<C: Signing>(
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
            .agent
            .post(&format!("{}/add-invoices", self.url))
            .send_json(payload);

        match resp {
            Ok(resp) => Ok(resp.into_json()?),
            Err(ureq::Error::Status(code, resp)) => {
                let str = resp.into_string().ok();
                Err(Error::HttpResponse(code, str))
            }
            Err(e) => Err(Error::Ureq(e)),
        }
    }
}
