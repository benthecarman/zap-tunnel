use std::str::FromStr;

use clap::Parser;
use url::Url;

#[derive(Parser, Debug, Clone)]
#[command(version, author, about)]
/// A tool to use zap-tunnel with lnd
pub struct Config {
    #[clap(default_value_t = Url::from_str("https://zaptunnel.com").unwrap(), long)]
    /// Host of the proxy server
    pub proxy_url: Url,
    #[clap(default_value_t = 20, long, short)]
    /// Number of invoices to cache on the proxy server
    pub invoice_cache: usize,
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

pub fn default_macaroon_file(network: String) -> String {
    format!(
        "{}/.lnd/data/chain/bitcoin/{}/admin.macaroon",
        home_directory(),
        network
    )
}
