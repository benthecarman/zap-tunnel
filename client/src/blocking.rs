//! LNURL by way of `ureq` HTTP client.
#![allow(clippy::result_large_err)]

use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey, Signing};
use lightning_invoice::Invoice;
use std::time::Duration;

use ureq::{Agent, Proxy};

use crate::{AddInvoices, Builder, CreateUser, CreateUserResponse, Error};

#[derive(Debug, Clone)]
pub struct BlockingClient {
    url: String,
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
            agent_builder = agent_builder.proxy(Proxy::new(proxy).unwrap());
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
        let pubkey = PublicKey::from_secret_key(context, &private_key);

        let signature =
            context.sign_ecdsa_low_r(&CreateUser::message_hash(&username).unwrap(), &private_key);

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
            Err(ureq::Error::Status(code, _)) => Err(Error::HttpResponse(code)),
            Err(e) => Err(Error::Ureq(e)),
        }
    }

    pub fn add_invoices<C: Signing>(
        &self,
        context: &Secp256k1<C>,
        private_key: &SecretKey,
        invoices: &[Invoice],
    ) -> Result<CreateUserResponse, Error> {
        let pubkey = PublicKey::from_secret_key(context, &private_key);

        let signature =
            context.sign_ecdsa_low_r(&AddInvoices::message_hash(invoices).unwrap(), &private_key);

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
            Err(ureq::Error::Status(code, _)) => Err(Error::HttpResponse(code)),
            Err(e) => Err(Error::Ureq(e)),
        }
    }
}
