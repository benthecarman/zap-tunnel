use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone, Deserialize, Serialize)]
#[command(version, author, about)]
/// A tool to use zap-tunnel with lnd
pub struct Config {
    #[clap(default_value_t = String::from("profiles.sled"), long)]
    /// Location of database file
    pub db_path: String,
    #[clap(default_value_t = 20, long, short)]
    /// Number of invoices to cache on the proxy server
    pub invoice_cache: usize,
    #[clap(default_value_t = String::from("Zap Tunnel"), long)]
    /// Memo in the invoices created for the zap tunnel.
    /// This is only for personal reference.
    pub invoice_memo: String,
    #[clap(default_value_t = String::from("127.0.0.1"), long)]
    /// Host of the GRPC server for lnd
    pub lnd_host: String,
    #[clap(default_value_t = 10009, long)]
    /// Port of the GRPC server for lnd
    pub lnd_port: u32,
    #[clap(default_value_t = String::from("mainnet"), short, long)]
    /// Network lnd is running on ["mainnet", "testnet", "signet, "simnet, "regtest"]
    pub network: String,
    #[clap(long)]
    /// Path to tls.cert file for lnd
    pub cert_file: Option<String>,
    #[clap(long)]
    /// Path to admin.macaroon file for lnd
    pub macaroon_file: Option<String>,
}

impl Config {
    /// Combine two configs into one
    /// This is used to combine the cmd line args with the config file
    /// The cmd line args take precedence, in this function
    /// other is the config file
    pub fn combine(mut self, other: Self) -> Self {
        // todo this doesn't properly handle defaults correctly
        if self.db_path.is_empty() {
            self.db_path = other.db_path;
        }
        if self.invoice_memo.is_empty() {
            self.invoice_memo = other.invoice_memo;
        }
        if self.lnd_host.is_empty() {
            self.lnd_host = other.lnd_host;
        }
        if self.network.is_empty() {
            self.network = other.network;
        }
        self.invoice_cache = self.invoice_cache.max(other.invoice_cache);
        self.lnd_port = if self.lnd_port == 10009 {
            other.lnd_port
        } else {
            self.lnd_port
        };
        self.cert_file = self.cert_file.or(other.cert_file);
        self.macaroon_file = self.macaroon_file.or(other.macaroon_file);

        self
    }
}

fn home_directory() -> String {
    let buf = home::home_dir().expect("Failed to get home dir");
    let str = format!("{}", buf.display());

    // to be safe remove possible trailing '/' and
    // we can manually add it to paths
    match str.strip_suffix('/') {
        Some(stripped) => stripped.to_string(),
        None => str,
    }
}

pub fn default_cert_file() -> String {
    format!("{}/.lnd/tls.cert", home_directory())
}

pub fn default_macaroon_file(network: &str) -> String {
    format!(
        "{}/.lnd/data/chain/bitcoin/{}/admin.macaroon",
        home_directory(),
        network
    )
}
