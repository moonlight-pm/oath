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
        if !valid_kind(kind) || !valid_name(name) {
            return Err(Error::hint(
                format!("not an id: {s}"),
                "ids are kind:name; kind is a-z, name is letters, digits, \
                 `.` `_` `-`, at most 64, not starting with `.` — try `oath ls`",
            ));
        }
        Ok(Self::new(kind, name))
    }
}

fn valid_kind(kind: &str) -> bool {
    !kind.is_empty() && kind.len() <= 16 && kind.bytes().all(|b| b.is_ascii_lowercase())
}

/// Names become directory names under `/oath/objects/<kind>/`, so they
/// can never climb out of it.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_grammar() {
        for ok in ["svc:sshd", "dev:ttyS0", "snap:12", "pkg:pkg-config", "svc:a.b_c"] {
            assert!(ok.parse::<ObjectId>().is_ok(), "{ok}");
        }
        for bad in [
            "svc",
            "svc:",
            ":x",
            "svc:../etc",
            "svc:..",
            "svc:.hidden",
            "svc:a/b",
            "SVC:x",
            "svc:a b",
            "svc:a:b",
        ] {
            assert!(bad.parse::<ObjectId>().is_err(), "{bad}");
        }
        assert!(format!("svc:{}", "a".repeat(65)).parse::<ObjectId>().is_err());
    }
}
