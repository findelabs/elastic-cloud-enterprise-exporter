use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct AllocatorsRoot {
    pub zones: Vec<Zone>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Zone {
    pub zone_id: String,
    pub allocators: Vec<Allocator>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Status {
    pub connected: bool,
    pub healthy: bool,
    pub maintenance_mode: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Capacity {
    pub memory: Memory
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Memory {
    pub total: u64,
    pub used: u64
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlansInfo {
    pub pending: bool,
    pub version: String,
    pub zone_count: Option<u64>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Instance {
    pub cluster_type: String,
    pub cluster_id: String,
    pub cluster_name: String,
    pub instance_name: String,
    pub node_memory: u64,
    pub healthy: bool,
    pub cluster_healthy: bool,
    pub instance_configuration_id: String,
    pub moving: bool,
    pub plans_info: PlansInfo,
    pub deployment_id: String
}
#[derive(Serialize, Deserialize, Debug)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct BuildInfo {
    pub commit_hash: String,
    pub version: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct ExternalLink {
    pub id: String,
    pub label: String,
    pub uri: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Allocator {
    pub status: Status,
    pub allocator_id: String,
    pub zone_id: String,
    pub host_ip: String,
    pub public_hostname: String,
    pub capacity: Capacity,
    pub settings: HashMap<String, Value>,
    pub instances: Vec<Instance>,
    pub metadata: Vec<KeyValue>,
    pub build_info: BuildInfo,
    pub features: Vec<String>,
    pub external_links: Vec<ExternalLink>
}
