use clap::ArgMatches;
use std::error::Error;
use hyper::{Body, Request, Response};
use http_auth_basic::Credentials;
use hyper::header::AUTHORIZATION;
use hyper::header::HeaderValue;

use crate::https::{HttpsClient, ClientBuilder};
use crate::error::Error as RestError;
use crate::allocator;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub client: HttpsClient,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>
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

        let client = ClientBuilder::new().timeout(timeout).build()?;

        Ok(State {
            client,
            url: opts.value_of("url").unwrap().to_string(),
            username: opts.value_of("username").map(str::to_string),
            password: opts.value_of("password").map(str::to_string),
            api_key: opts.value_of("apikey").map(str::to_string),
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
            log::info!("Adding authorization header: {}", &value);
            let header = HeaderValue::from_str(&value).expect("failed to convert credential header");
            headers.insert(AUTHORIZATION, header);
        } else {
            let credentials = Credentials::new(&self.username.as_ref().unwrap(), &self.password.as_ref().unwrap());
            let credentials = credentials.as_http_header();
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&credentials).expect("failed to convert credential header"));
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
            200 => {
                Ok(response)
            }
            _ => {
                log::error!(
                    "Got bad status code getting config: {}",
                    response.status().as_u16()
                );
                return Err(RestError::UnknownCode)
            }
        }
    }

    pub async fn get_allocators(&self) -> Result<allocator::AllocatorsRoot, RestError> {
        let body = self.get("api/v1/platform/infrastructure/allocators").await?;
        let bytes = hyper::body::to_bytes(body.into_body()).await?;
        let value: allocator::AllocatorsRoot= serde_json::from_slice(&bytes)?;
        Ok(value)
    }

    pub async fn parse_allocators(&self) -> Result<(), RestError> {
        let body = self.get_allocators().await?;
        log::debug!("{:#?}", body);

        for zone in body.zones {
            log::debug!("\"Working in zone: {}\"", zone.zone_id);
            for allocator in zone.allocators {
                log::debug!("\"Working in allocator: {}\"", allocator.public_hostname);
                let labels = [
                    ("zone", zone.zone_id.clone()),
                    ("hostname", allocator.public_hostname.to_owned()),
                    ("connected", allocator.status.connected.to_string()),
                    ("healthy", allocator.status.healthy.to_string()),
                    ("maintenance", allocator.status.maintenance_mode.to_string()),
                ];
                metrics::gauge!("ece_allocator_info", 1f64, &labels);

                let labels = [
                    ("zone", zone.zone_id.clone()),
                    ("hostname", allocator.public_hostname.to_owned()),
                ];
                metrics::gauge!("ece_allocator_memory_used", allocator.capacity.memory.used.clone() as f64, &labels);
                metrics::gauge!("ece_allocator_memory_total", allocator.capacity.memory.total.clone() as f64, &labels);
                metrics::gauge!("ece_allocator_instances_total", allocator.instances.len() as f64, &labels);

                for instance in allocator.instances {
                    let cluster_name = instance.cluster_name.unwrap_or("null".to_string()).to_owned();
                    log::debug!("\"Working in instance: {}\"", &cluster_name);
                    let labels = [
                        ("zone", zone.zone_id.clone()),
                        ("allocator", allocator.public_hostname.to_owned()),
                        ("name", cluster_name.clone()),
                        ("cluster_type", instance.cluster_type.to_string()),
                        ("cluster_id", instance.cluster_id.to_owned()),
                        ("configuration_id", instance.instance_configuration_id.to_owned()),
                        ("deployment_id", instance.deployment_id.unwrap_or("null".to_string()).to_owned()),
                        ("healthy", instance.healthy.unwrap_or(false).to_string()),
                        ("cluster_healthy", instance.cluster_healthy.unwrap_or(false).to_string()),
                        ("node_memory", instance.node_memory.to_string()),
                        ("moving", instance.moving.unwrap_or(false).to_string()),
                    ];
                    metrics::gauge!("ece_allocator_instance_info", 1f64, &labels);

                    if let Some(plans_info) = instance.plans_info {
                        let labels = [
                            ("zone", zone.zone_id.clone()),
                            ("allocator", allocator.public_hostname.to_owned()),
                            ("name", cluster_name.clone()),
                            ("pending", plans_info.pending.to_string()),
                            ("version", plans_info.version.to_owned()),
                            ("zone_count", plans_info.zone_count.unwrap_or(0u64).to_string()),
                        ];
                        metrics::gauge!("ece_allocator_instance_plan", 1f64, &labels);
                    }
                }
            }
        }
        Ok(())
    }    

    pub async fn get_metrics(&self) -> Result<(), RestError> {
        self.parse_allocators().await?;
        Ok(())
    }
}
