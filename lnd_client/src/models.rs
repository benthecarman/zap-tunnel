use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SetupUser {
    pub proxy: String,
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ViewStatus {
    pub proxy: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Status {
    pub proxy: String,
    pub username: String,
    pub invoices_remaining: u64,
}
