use chrono::DateTime;
use chrono::Datelike;
use chrono::NaiveDate;
use chrono::Utc;
use clap::ArgMatches;
use http_auth_basic::Credentials;
use hyper::header::HeaderValue;
use hyper::header::AUTHORIZATION;
use hyper::{Body, Request, Response};
use serde_json::Value;
use std::error::Error;

use crate::error::Error as RestError;
use crate::https::{ClientBuilder, HttpsClient};
use crate::{allocator, proxy};

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub client: HttpsClient,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
    pub eru_cost: u64,
}

impl State {
    pub async fn new(opts: ArgMatches) -> BoxResult<Self> {
        // Set timeout
        let timeout: u64 = opts
            .value_of("timeout")
            .unwrap()
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("Supplied timeout not in range, defaulting to 60");
                60
            });

        let eru_cost: u64 = opts
            .value_of("eru_cost")
            .unwrap()
            .parse()
            .unwrap_or_else(|_| {
                eprintln!("ERU cost is not with available range, defaulting to 6000");
                60
            });

        let client = ClientBuilder::new().timeout(timeout).build()?;

        Ok(State {
            client,
            url: opts.value_of("url").unwrap().to_string(),
            username: opts.value_of("username").map(str::to_string),
            password: opts.value_of("password").map(str::to_string),
            api_key: opts.value_of("apikey").map(str::to_string),
            eru_cost,
        })
    }

    pub async fn get(&self, path: &str) -> Result<Response<Body>, RestError> {
        let uri = format!("{}/{}", &self.url, path);
        log::debug!("getting url {}", &uri);

        let mut req = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .expect("request builder");

        let headers = req.headers_mut();

        if let Some(api_key) = &self.api_key {
            let value = format!("ApiKey {}", api_key);
            log::debug!("Adding authorization header: {}", &value);
            let header =
                HeaderValue::from_str(&value).expect("failed to convert credential header");
            headers.insert(AUTHORIZATION, header);
        } else {
            let credentials = Credentials::new(
                &self.username.as_ref().unwrap(),
                &self.password.as_ref().unwrap(),
            );
            let credentials = credentials.as_http_header();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&credentials).expect("failed to convert credential header"),
            );
        };

        // Send initial request
        let response = match self.client.request(req).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("{{\"error\":\"{}\"", e);
                return Err(RestError::Hyper(e));
            }
        };

        match response.status().as_u16() {
            404 => return Err(RestError::NotFound),
            403 => return Err(RestError::Forbidden),
            401 => return Err(RestError::Unauthorized),
            200 => Ok(response),
            _ => {
                log::error!(
                    "Got bad status code from ECE: {}",
                    response.status().as_u16()
                );
                let bytes = hyper::body::to_bytes(response.into_body()).await?;
                let value: Value = serde_json::from_slice(&bytes)?;
                log::error!("Bad response body: {}", value);
                return Err(RestError::UnknownCode);
            }
        }
    }

    pub async fn get_allocators(&self) -> Result<allocator::AllocatorsRoot, RestError> {
        let body = self
            .get("api/v1/platform/infrastructure/allocators")
            .await?;
        let bytes = hyper::body::to_bytes(body.into_body()).await?;
        let value: allocator::AllocatorsRoot = serde_json::from_slice(&bytes)?;
        Ok(value)
    }

    pub async fn get_proxies(&self) -> Result<proxy::ProxiesRoot, RestError> {
        let body = self.get("api/v1/platform/infrastructure/proxies").await?;
        let bytes = hyper::body::to_bytes(body.into_body()).await?;
        let value: proxy::ProxiesRoot = serde_json::from_slice(&bytes)?;
        Ok(value)
    }

    pub async fn parse_proxies(&self) -> Result<(), RestError> {
        let body = self.get_proxies().await?;
        log::debug!("{:#?}", body);

        for proxy in body.proxies {
            log::debug!("\"Working on proxy: {}\"", proxy.proxy_id);
            let labels = [
                ("zone", proxy.zone.clone()),
                ("hostname", proxy.public_hostname.to_owned()),
                ("proxy_id", proxy.proxy_id.to_owned()),
                (
                    "proxy_ip",
                    proxy.proxy_ip.unwrap_or("null".to_string()).to_owned(),
                ),
                ("healthy", proxy.healthy.to_string()),
            ];
            metrics::gauge!("ece_proxy_info", 1f64, &labels);
        }
        Ok(())
    }

    pub async fn parse_allocators(&self) -> Result<(), RestError> {
        let body = self.get_allocators().await?;
        log::debug!("{:#?}", body);

        // Calculate seconds since month start
        let now = chrono::Utc::now();
        let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1u32)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let month_start_utc = DateTime::<Utc>::from_utc(month_start, Utc);
        let seconds_since_month_start =
            now.signed_duration_since(month_start_utc).num_seconds() as f64;

        log::debug!("\"Seconds in month: {}\"", seconds_since_month_start);

        // Cents per GB for current month
        let cents_per_gb_current_month: f64 =
            (self.eru_cost as f64 * 100.0 / 31536000.0) * seconds_since_month_start;
        log::debug!(
            "\"cents per gb for current month: {}\"",
            cents_per_gb_current_month
        );

        for zone in body.zones {
            log::debug!("\"Working in zone: {}\"", zone.zone_id);
            for allocator in zone.allocators {
                log::debug!("\"Working in allocator: {}\"", allocator.public_hostname);

                // Generate a set of standard labels for allocator
                let mut alloc_tags = Vec::new();
                for tag in &allocator.metadata {
                    let key = tag.key.to_owned();
                    alloc_tags.push((key, tag.value.clone()))
                }

                let mut labels = vec![
                    ("zone".to_string(), zone.zone_id.clone()),
                    ("ip".to_string(), allocator.public_hostname.to_owned()),
                    (
                        "connected".to_string(),
                        allocator.status.connected.to_string(),
                    ),
                    ("healthy".to_string(), allocator.status.healthy.to_string()),
                    (
                        "maintenance".to_string(),
                        allocator.status.maintenance_mode.to_string(),
                    ),
                ];

                // Include allocator tags
                for tag in &alloc_tags {
                    labels.push(tag.clone())
                }

                metrics::gauge!("ece_allocator_info", 1f64, &labels);

                let mut labels = vec![
                    ("zone".to_string(), zone.zone_id.clone()),
                    ("ip".to_string(), allocator.public_hostname.to_owned()),
                ];

                // Include allocator tags
                for tag in &alloc_tags {
                    labels.push(tag.clone())
                }

                metrics::gauge!(
                    "ece_allocator_memory_used",
                    allocator.capacity.memory.used.clone() as f64,
                    &labels
                );
                metrics::gauge!(
                    "ece_allocator_memory_total",
                    allocator.capacity.memory.total.clone() as f64,
                    &labels
                );
                metrics::gauge!(
                    "ece_allocator_instances_total",
                    allocator.instances.len() as f64,
                    &labels
                );

                for instance in allocator.instances {
                    let cluster_name = instance
                        .cluster_name
                        .unwrap_or("null".to_string())
                        .to_owned();
                    let cluster_healthy = match instance.cluster_healthy {
                        Some(t) => t.to_string(),
                        None => "null".to_string(),
                    };
                    log::debug!("\"Working in instance: {}\"", &cluster_name);
                    let mut labels = vec![
                        ("zone".to_string(), zone.zone_id.clone()),
                        ("ip".to_string(), allocator.public_hostname.to_owned()),
                        ("name".to_string(), cluster_name.clone()),
                        (
                            "cluster_type".to_string(),
                            instance.cluster_type.to_string(),
                        ),
                        ("cluster_id".to_string(), instance.cluster_id.to_owned()),
                        (
                            "configuration_id".to_string(),
                            instance.instance_configuration_id.to_owned(),
                        ),
                        (
                            "deployment_id".to_string(),
                            instance
                                .deployment_id
                                .unwrap_or("null".to_string())
                                .to_owned(),
                        ),
                        (
                            "healthy".to_string(),
                            instance.healthy.unwrap_or(false).to_string(),
                        ),
                        ("cluster_healthy".to_string(), cluster_healthy.to_owned()),
                        (
                            "moving".to_string(),
                            instance.moving.unwrap_or(false).to_string(),
                        ),
                    ];

                    // Include allocator tags
                    for tag in &alloc_tags {
                        labels.push(tag.clone())
                    }
                    metrics::gauge!("ece_allocator_instance_info", 1f64, &labels);

                    let mut labels = vec![
                        ("zone".to_string(), zone.zone_id.clone()),
                        ("ip".to_string(), allocator.public_hostname.to_owned()),
                        ("name".to_string(), cluster_name.clone()),
                        (
                            "cluster_type".to_string(),
                            instance.cluster_type.to_string(),
                        ),
                        ("cluster_id".to_string(), instance.cluster_id.to_owned()),
                    ];
                    // Include allocator tags
                    for tag in &alloc_tags {
                        labels.push(tag.clone())
                    }
                    metrics::gauge!(
                        "ece_allocator_instance_node_memory",
                        instance.node_memory.clone() as f64,
                        &labels
                    );

                    // Size of cluster in GB: {{ Cluster size in MB }} / 1024
                    let cluster_size_gb: f64 = instance.node_memory as f64 / 1024.0;

                    let cluster_cost_over_month =
                        (cluster_size_gb / 64.0) * cents_per_gb_current_month;

                    // Get instance cost per month
                    metrics::gauge!(
                        "ece_allocator_instance_monthly_cost",
                        cluster_cost_over_month as f64,
                        &labels
                    );

                    if let Some(plans_info) = instance.plans_info {
                        let mut labels = vec![
                            ("zone".to_string(), zone.zone_id.clone()),
                            (
                                "allocator".to_string(),
                                allocator.public_hostname.to_owned(),
                            ),
                            ("name".to_string(), cluster_name.clone()),
                            ("pending".to_string(), plans_info.pending.to_string()),
                            (
                                "version".to_string(),
                                plans_info.version.unwrap_or("0".to_string()).to_owned(),
                            ),
                            (
                                "cluster_type".to_string(),
                                instance.cluster_type.to_string(),
                            ),
                            (
                                "zone_count".to_string(),
                                plans_info.zone_count.unwrap_or(0u64).to_string(),
                            ),
                        ];
                        // Include allocator tags
                        for tag in &alloc_tags {
                            labels.push(tag.clone())
                        }
                        metrics::gauge!("ece_allocator_instance_plan", 1f64, &labels);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_metrics(&self) -> Result<(), RestError> {
        self.parse_allocators().await?;
        self.parse_proxies().await?;
        Ok(())
    }
}
