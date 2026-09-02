use crate::id::ObjectId;
use crate::kinds::{Dev, DevActual, Host, Net, Pkg, PkgActual, Ssh, SshActual};
use crate::Result;

#[derive(Clone, Debug)]
pub struct Actor {
    pub uid: u32,
    pub tty: String,
}

impl Actor {
    pub fn unknown() -> Self {
        Self { uid: 0, tty: "unknown".into() }
    }

    pub fn current() -> Self {
        let uid = libc_uid();
        let tty = std::fs::read_link("/proc/self/fd/0")
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .filter(|s| s.starts_with("/dev/") || s.contains("tty") || s.contains("pts"))
            .unwrap_or_else(|| "unknown".into());
        Self { uid, tty }
    }
}

fn libc_uid() -> u32 {
    #[cfg(unix)]
    {
        extern "C" {
            fn getuid() -> u32;
        }
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

#[derive(Clone, Debug, Default)]
pub struct ApplyReport {
    pub generation: u64,
    pub ids: Vec<String>,
    pub rebooting: bool,
    pub halting: bool,
}

pub trait ApplyHooks {
    fn snapshot(&self, generation: u64) -> Result<()>;
    fn restore_snapshot(&self, generation: u64) -> Result<()>;
    fn converge_host(&self, desired: &Host) -> Result<Host>;
    fn notify_init(&self) -> Result<()>;
    fn reboot(&self) -> Result<()>;
    fn halt(&self) -> Result<()>;
    /// After notify, wait until svc actual matches `enabled`. Default: no-op.
    fn wait_converge(&self, _id: &ObjectId, _enabled: bool) -> Result<()> {
        Ok(())
    }
    fn converge_pkg(&self, _id: &ObjectId, desired: &Pkg) -> Result<PkgActual> {
        Ok(PkgActual {
            present: desired.present,
            links: Vec::new(),
            removable: true,
            url: desired.url.clone(),
        })
    }
    fn converge_net(&self, _id: &ObjectId, desired: &Net) -> Result<Net> {
        Ok(desired.clone())
    }
    fn converge_ssh(&self, _id: &ObjectId, desired: &Ssh) -> Result<SshActual> {
        Ok(SshActual { authorized: desired.authorized.clone(), host_key: false })
    }
    fn converge_dev(&self, id: &ObjectId, desired: &Dev) -> Result<DevActual> {
        if !desired.present {
            return Err(crate::Error::hint(format!("{id} is not removable"), "oath schema dev"));
        }
        let (class, node) = match id.name.as_str() {
            "vda" => ("block", "/dev/vda"),
            "net0" => ("net", "/sys/class/net/net0"),
            "ttyS0" => ("tty", "/dev/ttyS0"),
            "card0" => ("drm", "/dev/dri/card0"),
            "kbd0" => ("input", "/dev/input/event0"),
            "mouse0" => ("input", "/dev/input/event1"),
            _ => ("unknown", ""),
        };
        Ok(DevActual { present: true, class: class.into(), node: node.into() })
    }
}

pub struct NullHooks;

impl ApplyHooks for NullHooks {
    fn snapshot(&self, _generation: u64) -> Result<()> {
        Ok(())
    }
    fn restore_snapshot(&self, _generation: u64) -> Result<()> {
        Ok(())
    }
    fn converge_host(&self, desired: &Host) -> Result<Host> {
        Ok(Host {
            hostname: desired.hostname.clone(),
            power: crate::kinds::HostPower::Run,
            env: desired.env.clone(),
        })
    }
    fn notify_init(&self) -> Result<()> {
        Ok(())
    }
    fn reboot(&self) -> Result<()> {
        Ok(())
    }
    fn halt(&self) -> Result<()> {
        Ok(())
    }
}
