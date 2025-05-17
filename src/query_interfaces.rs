use {
    crate::config::Config,
    network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig},
    std::net::IpAddr,
    tower::BoxError,
};

pub(crate) async fn get_addresses_from_network_interfaces(config: &Config) -> Result<Vec<IpAddr>, BoxError> {
    let mut result = Vec::with_capacity(16);

    let interfaces = NetworkInterface::show()?;
    for interface in interfaces {
        if config.allows_interface(&interface.name) {
            for addr in interface.addr {
                let addr = match addr {
                    Addr::V4(addr) => IpAddr::V4(addr.ip),
                    Addr::V6(addr) => IpAddr::V6(addr.ip),
                };

                if config.allows_address(&addr) {
                    result.push(addr);
                }
            }
        }
    }

    Ok(result)
}
