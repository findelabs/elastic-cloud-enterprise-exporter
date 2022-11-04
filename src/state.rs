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
    pub username: String,
    pub password: String
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
            username: opts.value_of("username").unwrap().to_string(),
            password: opts.value_of("password").unwrap().to_string(),
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
        let credentials = Credentials::new(&self.username, &self.password);
        let credentials = credentials.as_http_header();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&credentials).expect("failed to convert credential header"));
        
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
                    ("hostname", allocator.public_hostname.to_string()),
                    ("connected", allocator.status.connected.to_string()),
                    ("healthy", allocator.status.healthy.to_string()),
                    ("maintenance", allocator.status.maintenance_mode.to_string()),
                ];
                metrics::gauge!("ece_allocator_info", 1f64, &labels);

                let labels = [
                    ("zone", zone.zone_id.clone()),
                    ("hostname", allocator.public_hostname.to_string()),
                ];
                metrics::gauge!("ece_allocator_memory_used", allocator.capacity.memory.used.clone() as f64, &labels);
                metrics::gauge!("ece_allocator_memory_total", allocator.capacity.memory.total.clone() as f64, &labels);
                metrics::gauge!("ece_allocator_instances_total", allocator.instances.len() as f64, &labels);

                for instance in allocator.instances {
                    log::debug!("\"Working in instance: {}\"", instance.cluster_name);
                    let labels = [
                        ("zone", zone.zone_id.clone()),
                        ("allocator", allocator.public_hostname.to_string()),
                        ("name", instance.cluster_name.to_string()),
                        ("cluster_type", instance.cluster_type.to_string()),
                        ("cluster_id", instance.cluster_id.to_string()),
                        ("configuration_id", instance.instance_configuration_id.to_string()),
                        ("deployment_id", instance.deployment_id.to_string()),
                        ("healthy", instance.healthy.to_string()),
                        ("cluster_healthy", instance.cluster_healthy.to_string()),
                        ("node_memory", instance.node_memory.to_string()),
                        ("moving", instance.moving.to_string()),
                        ("pending", instance.plans_info.pending.to_string()),
                        ("version", instance.plans_info.version.to_string()),
                        ("zone_count", instance.plans_info.zone_count.unwrap_or(0u64).to_string()),
                    ];
                    metrics::gauge!("ece_allocator_instance_info", 1f64, &labels);
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
