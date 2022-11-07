use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ProxiesRoot {
    pub proxies_count: u64,
    pub proxies: Vec<Proxy>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Proxy {
    pub proxy_id: String,
    pub proxy_ip: Option<String>,
    pub public_hostname: String,
    pub healthy: bool,
    pub zone: String
}
