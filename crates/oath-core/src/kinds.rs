use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub safety: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_generation: Option<u64>,
}

impl Meta {
    pub fn new(kind: &str, name: &str, safety: &str) -> Self {
        Self {
            id: format!("{kind}:{name}"),
            kind: kind.to_string(),
            name: name.to_string(),
            safety: safety.to_string(),
            status: "in-sync".into(),
            last_generation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub hostname: String,
    pub power: HostPower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostPower {
    Run,
    Reboot,
    Halt,
}

impl HostPower {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Reboot => "reboot",
            Self::Halt => "halt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Svc {
    pub exec: Vec<String>,
    #[serde(default)]
    pub wants: Vec<String>,
    pub restart: SvcRestart,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SvcRestart {
    Never,
    Always,
    OnFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SvcActual {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<i32>,
    #[serde(default)]
    pub restarts: u32,
}
