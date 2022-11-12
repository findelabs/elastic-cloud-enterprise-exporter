# Elastic Cloud Enterprise Exporter

A simple Elastic Cloud Enterprise (ECE) prometheus exporter. 

The exporter is currently able to pull the ECE admin API to gather information on allocator and proxy node metrics. You can view the published docker images [here](https://hub.docker.com/repository/docker/findelabs/elastic-cloud-enterprise-exporter).

A grafana dashboard is also available to use within the dashboards folder.

### Usage

```
    -a, --apikey <apikey>        ECE API Key [env: ECE_APIKEY=]
    -h, --help                   Print help information
    -p, --password <password>    ECE Password [env: ECE_PASSWORD=]
    -P, --port <port>            Set port to listen on [env: ECE_PORT=] [default: 8080]
    -t, --timeout <timeout>      Set default global timeout [env: ECE_TIMEOUT=] [default: 60s]
    -u, --username <username>    ECE Username [env: ECE_USERNAME=]
    -U, --url <url>              ECE Base URL [env: ECE_URL=]
    -V, --version                Print version information
```

### Metrics

```
# TYPE ece_allocator_info gauge
# TYPE ece_allocator_instance_info gauge
# TYPE ece_allocator_instance_node_memory gauge
# TYPE ece_allocator_instance_plan gauge
# TYPE ece_allocator_instances_total gauge
# TYPE ece_allocator_memory_total gauge
# TYPE ece_allocator_memory_used gauge
# TYPE ece_proxy_info gauge
```
