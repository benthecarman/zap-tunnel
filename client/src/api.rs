use std::str::FromStr;
use std::time::SystemTime;

use anyhow::anyhow;
use bitcoin::hashes::sha256::Hash as Sha256;
use bitcoin::hashes::Hash;
use bitcoin::secp256k1::ecdsa::Signature;
use bitcoin::secp256k1::{Message, PublicKey, Secp256k1, Verification};
use lightning_invoice::Invoice as LnInvoice;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub pubkey: String,
    pub signature: String,
}

impl CreateUser {
    pub fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    pub fn signature(&self) -> anyhow::Result<Signature> {
        Ok(Signature::from_str(&self.signature)?)
    }

    pub fn message_hash(username: &str) -> anyhow::Result<Message> {
        let hash = Sha256::hash(username.as_bytes());

        Ok(Message::from_slice(&hash)?)
    }

    pub fn validate<C: Verification>(&self, context: &Secp256k1<C>) -> anyhow::Result<()> {
        if self.username.len() < 3 {
            return Err(anyhow!("Username must be at least 3 characters long"));
        }

        let pubkey = self.pubkey().map_err(|_| anyhow!("Invalid pubkey"))?;
        let signature = self.signature().map_err(|_| anyhow!("Invalid signature"))?;

        let msg = CreateUser::message_hash(&self.username)?;

        if context.verify_ecdsa(&msg, &signature, &pubkey).is_err() {
            return Err(anyhow!("Invalid signature"));
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CreateUserResponse {
    pub username: String,
    pub pubkey: PublicKey,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CheckUser {
    pub username: String,
    pub pubkey: String,
    pub invoices_remaining: u64,
}

impl CheckUser {
    pub fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    pub fn message_hash(current_time: u64) -> anyhow::Result<Message> {
        let str = format!("CheckZapTunnelUser-{}", current_time);
        let hash = Sha256::hash(str.as_bytes());

        Ok(Message::from_slice(&hash)?)
    }

    pub fn validate<C: Verification>(
        context: &Secp256k1<C>,
        time: u64,
        pubkey: &PublicKey,
        signature: &Signature,
    ) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now - time > 60 {
            return Err(anyhow!("Request expired"));
        } else if now + 60 < time {
            return Err(anyhow!("Request is in the future"));
        }

        let message_hash = Self::message_hash(time)?;

        if context
            .verify_ecdsa(&message_hash, signature, pubkey)
            .is_err()
        {
            return Err(anyhow!("Invalid signature"));
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AddInvoices {
    pub pubkey: String,
    pub signature: String,
    pub invoices: Vec<LnInvoice>,
}

impl AddInvoices {
    pub fn pubkey(&self) -> anyhow::Result<PublicKey> {
        Ok(PublicKey::from_str(&self.pubkey)?)
    }

    pub fn signature(&self) -> anyhow::Result<Signature> {
        Ok(Signature::from_str(&self.signature)?)
    }

    pub fn message_hash(invoices: &[LnInvoice]) -> anyhow::Result<Message> {
        let bytes: Vec<u8> = invoices.iter().fold(Vec::new(), |mut acc, x| {
            acc.extend(x.payment_hash().to_vec());
            acc
        });
        let hash = Sha256::hash(&bytes);

        Ok(Message::from_slice(&hash)?)
    }

    pub fn validate<C: Verification>(&self, context: &Secp256k1<C>) -> anyhow::Result<()> {
        let pubkey = self.pubkey().map_err(|_| anyhow!("Invalid pubkey"))?;
        let signature = self.signature().map_err(|_| anyhow!("Invalid signature"))?;

        let message_hash = Self::message_hash(&self.invoices)?;

        if context
            .verify_ecdsa(&message_hash, &signature, &pubkey)
            .is_err()
        {
            return Err(anyhow!("Invalid signature"));
        }

        Ok(())
    }
}
