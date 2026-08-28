mod cpio;
mod pack;
mod probe;
mod qemu;
mod tools;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "oath-make",
    bin_name = "cargo make",
    about = "Host build CLI: pack the QEMU image, run it, probe it.",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pack initramfs + btrfs qcow (needs sudo for loop-mount).
    Build,
    /// Interactive serial QEMU; writes build/runs/<id>/.
    Run,
    /// Scripted courage test + telemetry.
    Probe,
}

fn main() {
    if let Err(e) = real() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

fn real() -> Result<()> {
    let cli = Cli::parse();
    let root = util::repo_root();
    let out = util::out_dir(&root);
    match cli.cmd {
        Cmd::Build => {
            let tools = tools::load(&root)?;
            pack::build(&root, &out, &tools)?;
        }
        Cmd::Run => {
            let rc = qemu::run_interactive(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Probe => {
            let rc = probe::probe(&root, &out)?;
            std::process::exit(rc);
        }
    }
    Ok(())
}
