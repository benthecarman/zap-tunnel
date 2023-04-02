use bitcoin::Network;
use clap::Parser;
use nostr::key::{FromSkStr, XOnlyPublicKey};
use nostr::Keys;

#[derive(Parser, Debug, Clone)]
#[command(version, author, about)]
/// A tool for proxying LNURL pay addresses.
pub struct Config {
    #[clap(long)]
    /// Nostr Private Key, used to sign zap requests, encoded as hex or bech32
    nsec: String,
    /// Base fee, in millisatoshis, for routing payments
    #[clap(default_value_t = 1000, long)]
    pub base_fee: u64,
    /// Fee rate in percentage of payment amount, for routing payments
    #[clap(default_value_t = 1.0, long)]
    pub fee_rate: f64,
    #[clap(default_value_t = String::from("127.0.0.1"), long)]
    /// Host of the GRPC server for lnd
    pub lnd_host: String,
    #[clap(default_value_t = 10009, long)]
    /// Port of the GRPC server for lnd
    pub lnd_port: u32,
    #[clap(default_value_t = Network::Bitcoin, short, long)]
    /// Network lnd is running on ["bitcoin", "testnet", "signet, "regtest"]
    pub network: Network,
    #[clap(long)]
    /// Path to tls.cert file for lnd
    cert_file: Option<String>,
    #[clap(long)]
    /// Path to admin.macaroon file for lnd
    macaroon_file: Option<String>,
    #[clap(default_value_t = String::from("db.sqlite"), long)]
    /// Location of database file
    pub db_path: String,
    #[clap(default_value_t = String::from("0.0.0.0"), long)]
    /// Bind address for zap-tunnel's webserver
    pub bind: String,
    #[clap(default_value_t = 3000, long)]
    /// Port for zap-tunnel's webserver
    pub port: u16,
    #[clap(long)]
    /// Public URL for zap-tunnel's webserver (eg zaptunnel.com)
    pub public_url: String,
}

impl Config {
    pub fn nostr_keys(&self) -> Keys {
        Keys::from_sk_str(&self.nsec).expect("Failed to parse nsec key")
    }

    pub fn public_key(&self) -> XOnlyPublicKey {
        self.nostr_keys().public_key()
    }

    pub fn macaroon_file(&self) -> String {
        self.macaroon_file
            .clone()
            .unwrap_or_else(|| default_macaroon_file(&self.network))
    }

    pub fn cert_file(&self) -> String {
        self.cert_file.clone().unwrap_or_else(default_cert_file)
    }

    pub fn min_sendable(&self) -> u64 {
        1_000.max(self.base_fee)
    }

    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        Self {
            nsec: "nsec1f77xgphdtw7g9qdryer6md8wv4nxvj83vweaejz8e8g7zgr2wttsxkmmfm".to_string(),
            base_fee: 1000,
            fee_rate: 1.0,
            lnd_host: "127.0.0.1".to_string(),
            lnd_port: 10009,
            network: Network::Regtest,
            cert_file: None,
            macaroon_file: None,
            db_path: "db.sqlite".to_string(),
            bind: "0.0.0.0".to_string(),
            port: 3000,
            public_url: "localhost".to_string(),
        }
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

pub fn default_macaroon_file(network: &Network) -> String {
    let network_str = match network {
        Network::Bitcoin => "mainnet",
        Network::Testnet => "testnet",
        Network::Signet => "signet",
        Network::Regtest => "regtest",
    };

    format!(
        "{}/.lnd/data/chain/bitcoin/{}/admin.macaroon",
        home_directory(),
        network_str
    )
}
