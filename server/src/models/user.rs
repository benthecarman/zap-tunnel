use super::schema::users;
use bitcoin::PublicKey;
use diesel::prelude::*;
use std::str::FromStr;

#[derive(Queryable, AsChangeset, Debug, Clone, PartialEq)]
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

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub auth_key: &'a str,
}
