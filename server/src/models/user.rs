use std::str::FromStr;

use bitcoin::secp256k1::PublicKey;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use super::schema::users;

#[derive(Queryable, Insertable, AsChangeset, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[diesel(primary_key(username))]
pub struct User {
    pub username: String,
    auth_key: String,
}

impl User {
    pub fn new(username: String, auth_key: PublicKey) -> Self {
        Self {
            username,
            auth_key: auth_key.to_string(),
        }
    }

    pub fn auth_key(&self) -> PublicKey {
        PublicKey::from_str(&self.auth_key).unwrap()
    }
}
