use {
    hickory_resolver::{
        config::{LookupIpStrategy, ResolverConfig, ResolverOpts},
        name_server::TokioConnectionProvider,
        system_conf::read_system_conf,
        ResolveError, TokioResolver,
    },
    log::debug,
    once_cell::sync::Lazy,
    reqwest::{
        dns::{Addrs, Resolve, Resolving, Name},
        Client,
    },
    std::{
        error::Error,
        fmt::{Display, Formatter, Result as FmtResult},
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::Duration,
    },
    tower::BoxError,
};

const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

pub(crate) async fn get_address_from_ip_service(
    ip_service: &str,
    timeout: Duration,
    lookup_ip_strategy: LookupIpStrategy,
) -> Result<Vec<IpAddr>, BoxError> {
    let mut result = Vec::with_capacity(1);
    let resolver = Arc::new(QueryResolver::new(lookup_ip_strategy)?);
    let client = Client::builder().dns_resolver(resolver).timeout(timeout).user_agent(USER_AGENT).build()?;

    debug!("Querying IP service at {ip_service} using strategy {lookup_ip_strategy:?}");

    let response = client.get(ip_service.to_string()).send().await?.error_for_status()?;
    let text = response.text().await?;

    let ip: IpAddr = text.trim().parse()?;
    result.push(ip);

    Ok(result)
}

struct QueryResolver {
    wrapped: TokioResolver,
}

impl QueryResolver {
    fn new(lookup_ip_strategy: LookupIpStrategy) -> Result<Self, BoxError> {
        let (config, mut opts) = get_global_resolve_config()?;
        opts.ip_strategy = lookup_ip_strategy;
        let resolver =
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default()).with_options(opts).build();

        Ok(Self {
            wrapped: resolver,
        })
    }
}

impl Resolve for QueryResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let wrapped = self.wrapped.clone();
        Box::pin(resolve_name(wrapped, name))
    }
}

async fn resolve_name(resolver: TokioResolver, name: Name) -> Result<Addrs, BoxError> {
    let addrs = resolver.lookup_ip(name.as_str()).await?;
    let mut result = Vec::new();
    for addr in addrs.iter() {
        result.push(SocketAddr::new(addr, 0));
    }

    Ok(Box::new(result.into_iter()))
}

#[derive(Clone, Debug)]
struct ResolveConfigNotAvailable(String);

impl Display for ResolveConfigNotAvailable {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "Resolve config not available: {}", self.0)
    }
}

impl Error for ResolveConfigNotAvailable {}

impl From<&ResolveError> for ResolveConfigNotAvailable {
    fn from(error: &ResolveError) -> Self {
        Self(error.to_string())
    }
}

fn get_global_resolve_config() -> Result<(ResolverConfig, ResolverOpts), ResolveConfigNotAvailable> {
    let m = RESOLVE_CONFIG.lock().unwrap();

    match (*m).as_ref() {
        Ok((config, opts)) => Ok((config.clone(), opts.clone())),
        Err(e) => Err(e.into()),
    }
}

static RESOLVE_CONFIG: Lazy<Mutex<Result<(ResolverConfig, ResolverOpts), ResolveError>>> =
    Lazy::new(|| Mutex::new(read_system_conf()));
