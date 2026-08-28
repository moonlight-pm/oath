use crate::id::ObjectId;
use crate::kinds::Host;
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
        Ok(Host { hostname: desired.hostname.clone(), power: crate::kinds::HostPower::Run })
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
