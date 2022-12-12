use {
    crate::error::Route53IpUpdateError,
    serde::{Deserialize, Deserializer, Serialize, Serializer, de::{self, Visitor, Unexpected}},
    std::{
        fmt::{Display, Formatter, Result as FmtResult},
        str::FromStr,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Ttl(i64);

impl Ttl {
    pub const fn from_seconds(seconds: i64) -> Self {
        if seconds <= 0 {
            panic!("TTL must be positive");
        } else {
            Self(seconds)
        }
    }
}

impl From<Ttl> for i64 {
    fn from(ttl: Ttl) -> Self {
        ttl.0
    }
}

impl FromStr for Ttl {
    type Err = Route53IpUpdateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i64>() {
            Ok(ttl) if ttl > 0 => Ok(Self(ttl)),
            _ => Err(Route53IpUpdateError::InvalidTtl(s.to_string())),
        }
    }
}

impl Display for Ttl {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

impl<'de> Deserialize<'de> for Ttl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_i64(TtlVisitor)
    }
}

struct TtlVisitor;
impl<'de> Visitor<'de> for TtlVisitor {
    type Value = Ttl;

    fn expecting(&self, formatter: &mut Formatter) -> FmtResult {
        formatter.write_str("positive integer representing the time-to-live in seconds")
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        if value <= 0 {
            Err(E::invalid_value(Unexpected::Signed(value), &"positive integer"))
        } else {
            Ok(Ttl(value))
        }
    }
}

impl Serialize for Ttl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}