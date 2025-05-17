use {
    crate::{args::Args, error::Route53IpUpdateError, query_address_type::QueryAddressType, ttl::Ttl},
    serde::{Deserialize, Serialize},
    std::{net::IpAddr, time::Duration},
};

const DEFAULT_IP_SERVICE: &str = "https://ipinfo.kanga.org/";

#[derive(Debug, Default, Deserialize, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Config {
    /// Whether to use IPv4, IPv6, or both.
    #[serde(default = "QueryAddressType::default")]
    pub(crate) address_type: QueryAddressType,

    /// Whether non-routable addresses should be allowed to be used.
    #[serde(default = "Config::default_allow_nonroutable")]
    pub(crate) allow_nonroutable: bool,

    /// Whether interfaces should be queried for their addresses.
    #[serde(default = "Config::default_query_interfaces")]
    pub(crate) query_interfaces: bool,

    /// Whether the IP service should be queried.
    #[serde(default = "Config::default_query_ip_service")]
    pub(crate) query_ip_service: bool,

    /// Interfaces to ignore while querying.
    pub(crate) ignore_interfaces: Option<Vec<String>>,

    /// The service to query for the current IP address.
    #[serde(default = "Config::default_ip_service")]
    pub(crate) ip_service: String,

    /// The timeout to allow for the IP service to respond.
    #[serde(with = "humantime_serde", default = "Config::default_timeout")]
    pub(crate) timeout: Duration,

    /// The Route 53 zones to update.
    #[serde(default = "Vec::new")]
    pub(crate) route53_zones: Vec<Route53ZoneConfig>,

    /// The default TTL to use for all records.
    pub(crate) ttl: Option<Ttl>,
}

impl Config {
    pub(crate) fn default_allow_nonroutable() -> bool {
        false
    }

    pub(crate) fn default_query_ip_service() -> bool {
        true
    }

    pub(crate) fn default_query_interfaces() -> bool {
        false
    }

    pub(crate) fn default_ip_service() -> String {
        DEFAULT_IP_SERVICE.to_string()
    }

    pub(crate) fn default_timeout() -> Duration {
        Duration::from_secs(10)
    }

    /// Indicates whether the specified interface should be used.
    pub(crate) fn allows_interface(&self, interface: &str) -> bool {
        if let Some(ignore_interfaces) = &self.ignore_interfaces {
            !ignore_interfaces.contains(&interface.to_string())
        } else {
            true
        }
    }

    /// Indicates whether the specified address should be used.
    pub(crate) fn allows_address(&self, addr: &IpAddr) -> bool {
        if !addr.is_global() && !self.allow_nonroutable {
            false
        } else {
            self.address_type.allows_address(addr)
        }
    }

    /// Updates the configuration using the specified arguments from the command line.
    pub(crate) fn update_from_args(&mut self, args: Args) {
        if let Some(address_type) = args.address_type {
            self.address_type = address_type;
        }

        if let Some(allow_nonroutable) = args.allow_nonroutable {
            self.allow_nonroutable = allow_nonroutable;
        }

        if let Some(query_interfaces) = args.query_interfaces {
            self.query_interfaces = query_interfaces;
        }

        if let Some(query_ip_service) = args.query_ip_service {
            self.query_ip_service = query_ip_service;
        }

        match self.ignore_interfaces {
            None => self.ignore_interfaces = Some(args.ignore_interfaces.clone()),
            Some(ref mut interfaces) => interfaces.extend(args.ignore_interfaces.iter().cloned()),
        };

        if let Some(ip_service) = args.ip_service {
            self.ip_service = ip_service;
        }

        if let Some(timeout) = args.timeout {
            self.timeout = *timeout;
        }

        if let Some(ttl) = args.ttl {
            self.ttl = Some(ttl);
        }

        if let Some(zone_id) = args.route53_zone {
            // Get the zone config.
            let r53_zc = self.get_or_create_zone_config(&zone_id);

            // Set any hostnames needed.
            for hostname in args.hostnames {
                r53_zc.add_hostname(&hostname);
            }
        }
    }

    fn get_or_create_zone_config(&mut self, zone_id: &str) -> &mut Route53ZoneConfig {
        let pos = self.route53_zones.iter_mut().position(|r53_zc| r53_zc.zone_id == zone_id);

        // If it doesn't exist, add it.
        match pos {
            None => {
                let old_len = self.route53_zones.len();
                self.route53_zones.push(Route53ZoneConfig {
                    zone_id: zone_id.to_string(),
                    hostnames: Vec::new(),
                    ttl: self.ttl,
                });

                &mut self.route53_zones[old_len]
            }
            Some(pos) => &mut self.route53_zones[pos],
        }
    }

    pub(crate) fn check(&self) -> Result<(), Route53IpUpdateError> {
        let mut messages = Vec::new();

        if self.query_ip_service && self.ip_service.is_empty() {
            messages.push("The IP service cannot be empty if querying the IP service is enabled.".to_string());
        }

        if self.route53_zones.is_empty() {
            messages.push("No Route 53 zones have been configured.".to_string());
        } else {
            for r53_zc in &self.route53_zones {
                if r53_zc.hostnames.is_empty() {
                    messages.push(format!("No hostnames have been configured for zone {}.", r53_zc.zone_id));
                }
            }
        }

        if messages.is_empty() {
            Ok(())
        } else {
            Err(Route53IpUpdateError::InvalidConfig(messages))
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Route53ZoneConfig {
    pub(crate) zone_id: String,

    pub(crate) hostnames: Vec<HostnameConfig>,

    /// The default TTL to use for all records.
    pub(crate) ttl: Option<Ttl>,
}

impl Route53ZoneConfig {
    fn add_hostname(&mut self, hostname: &str) {
        // Does this hostname exist?
        if self.hostnames.iter().any(|h| h.get_hostname() == hostname) {
            return;
        }

        // Nope; add it.
        self.hostnames.push(HostnameConfig::HostnameOnly(hostname.to_string()));
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum HostnameConfig {
    HostnameOnly(String),
    HostnameAndTtl(HostnameAndTtlConfig),
}

impl HostnameConfig {
    #[inline]
    pub fn get_hostname(&self) -> &str {
        match self {
            HostnameConfig::HostnameOnly(hostname) => hostname,
            HostnameConfig::HostnameAndTtl(hostname_and_ttl) => &hostname_and_ttl.hostname,
        }
    }

    #[inline]
    pub fn get_ttl(&self) -> Option<Ttl> {
        match self {
            HostnameConfig::HostnameOnly(_) => None,
            HostnameConfig::HostnameAndTtl(hostname_and_ttl) => Some(hostname_and_ttl.ttl),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct HostnameAndTtlConfig {
    pub(crate) hostname: String,
    pub(crate) ttl: Ttl,
}
