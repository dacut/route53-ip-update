use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

#[derive(Debug)]
pub enum Route53IpUpdateError {
    InvalidConfig(Vec<String>),
    InvalidIpAddr(String),
    InvalidQueryAddressType(String),
    InvalidTtl(String),
    MissingExpectedAwsReplyField(String),
    UnexpectedRoute53Status(String),
    UnknownConfigFileExt(Option<String>),
}

impl Display for Route53IpUpdateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::InvalidConfig(messages) => write!(f, "Invalid configuration: {}", messages.join(" ")),
            Self::InvalidIpAddr(ip) => write!(f, "Invalid IP address: {ip}"),
            Self::InvalidQueryAddressType(qat) => write!(f, "Invalid query address type: {qat}"),
            Self::InvalidTtl(ttl) => write!(f, "Invalid TTL: {ttl}"),
            Self::MissingExpectedAwsReplyField(field) => write!(f, "AWS reply is missing expected field: {field}"),
            Self::UnexpectedRoute53Status(status) => write!(f, "Unepxected Route 53 change status reported: {status}"),
            Self::UnknownConfigFileExt(ext) => match ext {
                Some(ext) => write!(f, "Unknown extension for configuration file: {ext}"),
                None => write!(f, "Configuration file has no extension"),
            },
        }
    }
}

impl Error for Route53IpUpdateError {}
