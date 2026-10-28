//! ESP boot generations: the rotation is oath-core's (`oath update`
//! writes a machine's ESP with it too).

use anyhow::{bail, Result};

pub use oath_core::esp::*;

pub fn require_confirm(confirm: bool) -> Result<()> {
    if !confirm {
        bail!("refusing to write the ESP without --confirm");
    }
    Ok(())
}
