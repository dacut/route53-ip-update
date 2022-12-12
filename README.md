# route53-ip-update
Update an AWS Route 53 DNS record with your IPv4 and/or IPv6 public address

# Usage

`route53-ip-update [OPTIONS] [HOSTNAMES]...`

## Arguments
`  [HOSTNAMES]...` The hostnames to update

## Options

* `-a`, `--address-type <ADDRESS_TYPE>`  
    Whether to use IPv4, IPv6, or both. If unspecified on the command-line and config file, defaults to both.
* `-n`, `--allow-nonroutable <ALLOW_NONROUTABLE>`  
    Whether non-routable addresses should be allowed to be used. If unspecified on the command-line and config
    file, defaults to false [possible values: true, false].
* `-c`, `--config-file <CONFIG_FILE>`  
    The config file to read, if any.
* `-q`, `--query-interfaces <QUERY_INTERFACES>`  
    Whether interfaces should be queried for their addresses. If unspecified on the command-line and config
    file, defaults to false [possible values: true, false].
* `-Q`, `--query-ip-service <QUERY_IP_SERVICE>`  
    Whether the IP service should be queried for the current IP address. If unspecified on the command-line and
    config file, defaults to true [possible values: true, false].
* `-I`, `--ignore-interfaces <IGNORE_INTERFACES>`  
    Interfaces to ignore while querying.
* `-s`, `--ip-service <IP_SERVICE>`  
    The service to query for the current IP address. If unspecified on the command-line and config file,
    defaults to `https://api64.ipify.org`.
* `-t`, `--timeout <TIMEOUT>`  
    The timeout to allow for the IP service to respond. If unspecified on the command-line and config file defaults to 10 seconds. This may be specified as a duration with units, e.g. 10s, 1m, etc.
* `-T`, `--ttl <TTL>`  
    The time-to-live to apply to new records, in seconds.
* `-r`, `--route53-zone <ROUTE53_ZONE>`  
    The Route 53 zone to update. If you need to update more than one Route 53 zone, use the config file
* `-h`, `--help`  
    Print help information.
* `-V`, `--version`  
    Print version information.

# Configuration file

The configuration file may be in TOML, YAML, or JSON format. The parser used is deteremined by the extension (`.toml` uses TOML; `.yaml`, `.yml`, and `.json` use the YAML parser, which is JSON-compatible).

The format of the configuration file is as follows (YAML):

```yaml
address-type: ipv4|ipv6|both   # Types of addresses to include
allow-nonroutable: false|true  # Whether non-routable records should be allowed
query-interfaces: false|true   # Whether interfaces should be queried
query-ip-service: false|true   # Whether the IP service should be queried
ignore-interfaces:             # List of interfaces to ignore while querying
  - interface-name
ip-service: https://hostname/  # IP service to query.
timeout: "10 s"                # Timeout for the IP service
ttl: 60                        # TTL in seconds to default to
route53-zones:                 # List of Route 53 zones
  - zone-id: zone1-id          # The Route 53 zone id
    ttl: 60                    # TTL in seconds to default to
    hostnames:                 # List of hostnames to update
      - hostname: host.net     # Hostname to update
        ttl: 10                # TTL in seconds to use for this record
  - zone-id: zone2-id
    hostnames:                 # Simplified way of specifying hostnames without TTL
      - host.net
```