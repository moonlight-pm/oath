use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    pub kind: String,
    pub name: String,
}

impl ObjectId {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self { kind: kind.into(), name: name.into() }
    }

    pub fn as_str(&self) -> String {
        format!("{}:{}", self.kind, self.name)
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.name)
    }
}

impl FromStr for ObjectId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let Some((kind, name)) = s.split_once(':') else {
            return Err(Error::hint(
                format!("not an id: {s}"),
                "ids look like kind:name — try `oath ls`",
            ));
        };
        if kind.is_empty() || name.is_empty() {
            return Err(Error::hint(
                format!("not an id: {s}"),
                "ids look like kind:name — try `oath ls`",
            ));
        }
        Ok(Self::new(kind, name))
    }
}
