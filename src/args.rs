use {
    crate::{config::Config, error::Route53IpUpdateError, query_address_type::QueryAddressType, ttl::Ttl},
    clap::{builder::ArgAction, Parser},
    humantime::Duration,
    std::path::Path,
    tokio::{fs::File, io::copy},
    tower::BoxError,
};

#[derive(Clone, Debug, Parser)]
#[command(name = "route53-ip-update", author, version, about, long_about = None)]
pub(crate) struct Args {
    /// Whether to use IPv4, IPv6, or both.
    #[arg(short = 'a', long = "address-type")]
    pub(crate) address_type: Option<QueryAddressType>,

    /// Whether non-routable addresses should be allowed to be used.
    #[arg(short = 'n', long = "allow-nonroutable")]
    pub(crate) allow_nonroutable: Option<bool>,

    /// The config file to read, if any.
    #[arg(short = 'c', long = "config-file")]
    pub config_file: Option<String>,

    /// Whether interfaces should be queried for their addresses.
    #[arg(short = 'q', long = "query-interfaces")]
    pub(crate) query_interfaces: Option<bool>,

    /// Interfaces to ignore while querying.
    #[arg(short = 'I', long = "ignore-interfaces", action = ArgAction::Append)]
    pub(crate) ignore_interfaces: Vec<String>,

    /// The service to query for the current IP address. To disable, set this to the empty string.
    #[arg(short = 's', long = "ip-service")]
    pub(crate) ip_service: Option<String>,

    /// The timeout to allow for the IP service to respond.
    #[arg(short = 't', long = "timeout")]
    pub(crate) timeout: Option<Duration>,

    /// The time-to-live to apply to new records.
    #[arg(short = 'T', long = "ttl")]
    pub(crate) ttl: Option<Ttl>,

    /// The Route 53 zone to update.
    #[arg(short = 'r', long = "route53-zone")]
    pub(crate) route53_zone: Option<String>,

    /// The hostnames to update.
    pub(crate) hostnames: Vec<String>,
}

impl Args {
    pub async fn into_config(self) -> Result<Config, BoxError> {
        let mut config = if let Some(config_file) = &self.config_file {
            let config_path = Path::new(config_file);
            let mut file = File::open(&config_path).await?;
            let ext = match config_path.extension() {
                None => None,
                Some(ext) => ext.to_str(),
            };
            let Some(ext) = ext else {
                return Err(Route53IpUpdateError::UnknownConfigFileExt(None).into());
            };

            match ext {
                "toml" => {
                    let mut file_contents = Vec::new();
                    copy(&mut file, &mut file_contents).await?;
                    toml::from_slice::<Config>(&file_contents)?
                }
                "json" | "yaml" | "yml" => {
                    let mut file_contents = Vec::new();
                    copy(&mut file, &mut file_contents).await?;
                    serde_yaml::from_slice::<Config>(&file_contents)?
                }
                _ => {
                    return Err(Route53IpUpdateError::UnknownConfigFileExt(Some(ext.to_string())).into());
                }
            }
        } else {
            Config::default()
        };

        config.update_from_args(self);
        Ok(config)
    }
}
