use {
    hyper::client::connect::dns::Name,
    log::debug,
    once_cell::sync::Lazy,
    reqwest::{
        dns::{Addrs, Resolve, Resolving},
        Client,
    },
    std::{
        error::Error,
        fmt::{Display, Formatter, Result as FmtResult},
        io::Error as IoError,
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::Duration,
    },
    tower::BoxError,
    trust_dns_proto::xfer::dns_handle::DnsHandle,
    trust_dns_resolver::{
        config::{LookupIpStrategy, ResolverConfig, ResolverOpts},
        error::ResolveError,
        name_server::{ConnectionProvider, GenericConnection, GenericConnectionProvider, TokioRuntime},
        system_conf::read_system_conf,
        AsyncResolver,
    },
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
    wrapped: AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>>,
}

impl QueryResolver {
    fn new(lookup_ip_strategy: LookupIpStrategy) -> Result<Self, BoxError> {
        let (config, mut opts) = get_global_resolve_config()?;
        opts.ip_strategy = lookup_ip_strategy;
        let resolver = AsyncResolver::tokio(config, opts)?;

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

async fn resolve_name<C, P>(resolver: AsyncResolver<C, P>, name: Name) -> Result<Addrs, BoxError>
where
    C: DnsHandle<Error = ResolveError>,
    P: ConnectionProvider<Conn = C>,
{
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

impl From<&IoError> for ResolveConfigNotAvailable {
    fn from(error: &IoError) -> Self {
        Self(error.to_string())
    }
}

fn get_global_resolve_config() -> Result<(ResolverConfig, ResolverOpts), ResolveConfigNotAvailable> {
    let m = RESOLVE_CONFIG.lock().unwrap();

    match (*m).as_ref() {
        Ok((config, opts)) => Ok((config.clone(), *opts)),
        Err(e) => Err(e.into()),
    }
}

static RESOLVE_CONFIG: Lazy<Mutex<Result<(ResolverConfig, ResolverOpts), IoError>>> =
    Lazy::new(|| Mutex::new(read_system_conf()));
