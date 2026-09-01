mod cpio;
mod install;
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
    about = "Host build CLI: pack the QEMU image, run it, start/stop it.",
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
    Run {
        /// Pack the image first (sudo for the loop-mount).
        #[arg(long)]
        build: bool,
    },
    /// Headless QEMU in the foreground. Serial in the run log. Ctrl-C kills the VM.
    Up {
        /// Pack the image first (sudo for the loop-mount).
        #[arg(long)]
        build: bool,
    },
    /// Headless QEMU in the background. Serial in the run log.
    Start {
        /// Pack the image first (sudo for the loop-mount).
        #[arg(long)]
        build: bool,
    },
    /// Stop a VM started with `start`.
    Stop,
    /// SSH to the QEMU guest (`root@127.0.0.1`, port `OATH_SSH_PORT` / 2222).
    Ssh {
        /// Extra ssh(1) args (`-i key`, a remote command, …).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Scripted courage test + telemetry.
    Probe,
    /// Wipe `--disk` on `--target` (or `--qemu` OVMF rehearsal). Requires `--confirm`.
    Install {
        /// `user@host` or `user@host:port`. Ignored with `--qemu`.
        #[arg(long)]
        target: Option<String>,
        /// Whole-disk node to GPT-wipe (`/dev/sda`, `/dev/vda`, …).
        #[arg(long)]
        disk: String,
        /// Required. This is a destructive confirm-class action.
        #[arg(long)]
        confirm: bool,
        /// Rehearse in QEMU+OVMF instead of a real host.
        #[arg(long)]
        qemu: bool,
        /// Write an EFI installer to `--disk` (must be a removable USB). No `--target`.
        #[arg(long)]
        usb: bool,
        /// `host:local` hostname after install (default: oath; canto: pass canto).
        #[arg(long)]
        hostname: Option<String>,
    },
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
        Cmd::Build => pack_image(&root, &out)?,
        Cmd::Run { build } => {
            if build {
                pack_image(&root, &out)?;
            }
            let rc = qemu::run_interactive(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Up { build } => {
            if build {
                pack_image(&root, &out)?;
            }
            let rc = qemu::run_up(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Start { build } => {
            if build {
                pack_image(&root, &out)?;
            }
            qemu::start(&root, &out)?;
        }
        Cmd::Stop => qemu::stop(&out)?,
        Cmd::Ssh { args } => {
            let rc = qemu::ssh(&out, &args)?;
            std::process::exit(rc);
        }
        Cmd::Probe => {
            let rc = probe::probe(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Install { target, disk, confirm, qemu, usb, hostname } => {
            install::run_install(
                &root,
                &out,
                install::Opts { target, disk, confirm, qemu, usb, hostname },
            )?;
        }
    }
    Ok(())
}

fn pack_image(root: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let tools = tools::load(root)?;
    pack::build(root, out, &tools)
}
