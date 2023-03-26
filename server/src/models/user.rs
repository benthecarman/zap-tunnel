#[cfg(test)]
use std::str::FromStr;

use bitcoin::secp256k1::PublicKey;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use super::schema::users;

#[derive(Queryable, Insertable, AsChangeset, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[diesel(primary_key(username))]
pub struct User {
    pub username: String,
    pubkey: String,
}

impl User {
    pub fn new(username: &str, pubkey: PublicKey) -> Self {
        Self {
            username: String::from(username),
            pubkey: pubkey.to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn pubkey(&self) -> PublicKey {
        PublicKey::from_str(&self.pubkey).expect("invalid pubkey")
    }

    pub fn get_by_username(conn: &mut SqliteConnection, username: &str) -> Option<Self> {
        users::table
            .filter(users::username.eq(username))
            .first::<Self>(conn)
            .ok()
    }

    pub fn get_by_pubkey(conn: &mut SqliteConnection, pubkey: &str) -> Option<Self> {
        users::table
            .filter(users::pubkey.eq(pubkey))
            .first::<Self>(conn)
            .ok()
    }
}
