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
    pub fn new(username: &str, auth_key: PublicKey) -> Self {
        Self {
            username: String::from(username),
            auth_key: auth_key.to_string(),
        }
    }

    pub fn auth_key(&self) -> PublicKey {
        PublicKey::from_str(&self.auth_key).unwrap()
    }

    pub fn get_by_username(conn: &mut SqliteConnection, username: &str) -> Option<Self> {
        users::table
            .filter(users::username.eq(username))
            .first::<Self>(conn)
            .ok()
    }

    pub fn get_by_auth_key(conn: &mut SqliteConnection, auth_key: &str) -> Option<Self> {
        users::table
            .filter(users::auth_key.eq(auth_key))
            .first::<Self>(conn)
            .ok()
    }
}
