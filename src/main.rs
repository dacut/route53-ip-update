#![feature(ip)]
#![warn(clippy::all)]

mod args;
mod config;
mod error;
mod query_address_type;
mod query_interfaces;
mod query_ip_service;
mod ttl;
mod update;

use {
    args::Args,
    aws_config::load_from_env as load_aws_config_from_env,
    aws_sdk_route53::Client as Route53Client,
    clap::Parser,
    futures::stream::{futures_unordered::FuturesUnordered, StreamExt},
    log::info,
    query_address_type::QueryAddressType,
    query_interfaces::get_addresses_from_network_interfaces,
    query_ip_service::get_address_from_ip_service,
    std::{collections::HashSet, future::Future, net::IpAddr, pin::Pin, process::ExitCode},
    tower::BoxError,
    trust_dns_resolver::config::LookupIpStrategy,
    update::update_zone,
};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    env_logger::init();
    let args = Args::parse();
    let config = match args.into_config().await {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = config.check() {
        eprintln!("Error: {e}");
        return ExitCode::FAILURE;
    }

    type IpQueryResult = Result<Vec<IpAddr>, BoxError>;
    let mut f: FuturesUnordered<Pin<Box<dyn Future<Output = IpQueryResult>>>> = FuturesUnordered::new();

    // If we're querying interfaces, add that to the futures.
    if config.query_interfaces {
        f.push(Box::pin(get_addresses_from_network_interfaces(&config)));
    }

    // If we're querying an IP service, add the IPv4 and/or IPv6 queries to the futures.
    if config.query_ip_service {
        if config.address_type == QueryAddressType::Both || config.address_type == QueryAddressType::Ipv4 {
            f.push(Box::pin(get_address_from_ip_service(
                &config.ip_service,
                config.timeout,
                LookupIpStrategy::Ipv4Only,
            )));
        }

        if config.address_type == QueryAddressType::Both || config.address_type == QueryAddressType::Ipv6 {
            f.push(Box::pin(get_address_from_ip_service(
                &config.ip_service,
                config.timeout,
                LookupIpStrategy::Ipv6Only,
            )));
        }
    }

    if f.is_empty() {
        eprintln!("Error: Not querying any interfaces or IP services.");
        return ExitCode::FAILURE;
    }

    let mut ipv4_addresses = HashSet::<IpAddr>::new();
    let mut ipv6_addresses = HashSet::<IpAddr>::new();
    let mut errors_found = false;

    while let Some(result) = f.next().await {
        match result {
            Ok(addresses) => {
                for address in addresses {
                    if config.allows_address(&address) {
                        match address {
                            IpAddr::V4(_) => ipv4_addresses.insert(address),
                            IpAddr::V6(_) => ipv6_addresses.insert(address),
                        };
                    }
                }
            }
            Err(err) => {
                eprintln!("Error: {err}");
                errors_found = true;
            }
        }
    }

    // Don't continue if we found any errors.
    if errors_found {
        return ExitCode::FAILURE;
    }

    let mut ipv4_addresses_sorted: Vec<&IpAddr> = ipv4_addresses.iter().collect();
    let mut ipv6_addresses_sorted: Vec<&IpAddr> = ipv6_addresses.iter().collect();
    ipv4_addresses_sorted.sort();
    ipv6_addresses_sorted.sort();

    let mut ipv4_address_strings = Vec::with_capacity(ipv4_addresses_sorted.len());
    let mut ipv6_address_strings = Vec::with_capacity(ipv6_addresses_sorted.len());

    for address in &ipv4_addresses_sorted {
        ipv4_address_strings.push(address.to_string());
    }

    for address in &ipv6_addresses_sorted {
        ipv6_address_strings.push(address.to_string());
    }

    info!("IPv4 addresses: {}", ipv4_address_strings.join(", "));
    info!("IPv6 addresses: {}", ipv6_address_strings.join(", "));

    let sdk_config = load_aws_config_from_env().await;
    let route53 = Route53Client::new(&sdk_config);

    let mut f = FuturesUnordered::new();
    for zone in &config.route53_zones {
        f.push(update_zone(route53.clone(), zone, config.ttl, &ipv4_addresses, &ipv6_addresses))
    }

    while let Some(result) = f.next().await {
        if let Err(e) = result {
            eprintln!("Error: {e}");
            errors_found = true;
        }
    }

    if errors_found {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
