use {
    crate::error::Route53IpUpdateError,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{Display, Formatter, Result as FmtResult},
        net::IpAddr,
        str::FromStr,
    },
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum QueryAddressType {
    Both,
    Ipv4,
    Ipv6,
}

impl QueryAddressType {
    pub fn allows_address(&self, addr: &IpAddr) -> bool {
        match addr {
            IpAddr::V4(_) => self == &Self::Both || self == &Self::Ipv4,
            IpAddr::V6(_) => self == &Self::Both || self == &Self::Ipv6,
        }
    }
}

impl Default for QueryAddressType {
    fn default() -> Self {
        Self::Both
    }
}

impl Display for QueryAddressType {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Both => write!(f, "both"),
            Self::Ipv4 => write!(f, "ipv4"),
            Self::Ipv6 => write!(f, "ipv6"),
        }
    }
}

impl FromStr for QueryAddressType {
    type Err = Route53IpUpdateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "both" => Ok(Self::Both),
            "ipv4" => Ok(Self::Ipv4),
            "ipv6" => Ok(Self::Ipv6),
            _ => Err(Route53IpUpdateError::InvalidQueryAddressType(s.to_string())),
        }
    }
}
