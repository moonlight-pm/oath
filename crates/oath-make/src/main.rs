mod advisories;
mod agent;
mod boot;
mod check;
mod cpio;
mod elf;
mod install;
mod kernel;
mod manifest;
mod oath_pkg;
mod openpgp;
mod output;
mod pack;
mod pkgs;
mod qemu;
mod sigstore;
mod tools;
mod update;
mod util;

use std::path::PathBuf;

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
    /// Pack the image into a build output, `build/out/<id>/` (needs sudo
    /// for the loop-mount). An unchanged tree reuses its output.
    Build {
        /// Seed profile (`base`: headless, reached over SSH).
        #[arg(long, default_value = oath_core::DEFAULT_PROFILE)]
        profile: String,
    },
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
    /// Boot the built image in QEMU and run check/cases.toml over SSH.
    /// A clean run is required before `install`. With `--target`, run the
    /// `metal` cases on an installed machine instead.
    Check {
        /// Pack the image first (sudo for the loop-mount).
        #[arg(long)]
        build: bool,
        /// Run only these cases.
        #[arg(long = "case")]
        only: Vec<String>,
        /// VMs at a time.
        #[arg(short, long, default_value_t = 4)]
        jobs: usize,
        /// `user@host[:port]` of a machine `install` wrote (its host key is
        /// pinned in build/known_hosts). Runs the `metal` cases, one at a time.
        #[arg(long)]
        target: Option<String>,
        /// Build output to boot (`build/out/<id>`; default: the latest).
        #[arg(long)]
        output: Option<String>,
        /// Give a real model only the machine's `oath mcp` and each task in
        /// check/agent.toml (`--case` picks tasks). Opt-in: needs
        /// ANTHROPIC_API_KEY, costs money, does not gate install.
        #[arg(long)]
        agent: bool,
        /// The model for `--agent` (default: check/agent.toml's).
        #[arg(long, requires = "agent")]
        model: Option<String>,
    },
    /// SSH to the QEMU guest (`home@127.0.0.1`, port `OATH_SSH_PORT` / 2222).
    Ssh {
        /// Extra ssh(1) args (`-i key`, a remote command, …).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
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
        /// `host:local` hostname after install (default: oath).
        #[arg(long)]
        hostname: Option<String>,
        /// Catalog overlay JSON (`{"objects": {"kind:name": {…}}}`) merged into
        /// the installed catalog. Machine-specific facts live here, not in the seed.
        #[arg(long)]
        overlay: Option<PathBuf>,
        /// Install even though `cargo make check` has not passed this image.
        #[arg(long)]
        unchecked: bool,
        /// Build output to install (`build/out/<id>`; default: the newest
        /// one `check` passed, or the latest with `--qemu` / `--unchecked`).
        #[arg(long)]
        output: Option<String>,
    },
    /// Build the toolchain from source on this host (stage 0 and 1) and
    /// store the base packs (pkg:gcc, pkg:glibc and the GNU tools). Hours, not minutes.
    Bootstrap {
        /// Parallel jobs (default: half the cores).
        #[arg(short, long)]
        jobs: Option<usize>,
    },
    /// Build native packages from pkgs/ as sandboxed `oath build` plans,
    /// in dependency order. No names: every package. `oath` builds
    /// pkg:oath from this tree (`cargo make build` does that itself).
    Pkgs {
        names: Vec<String>,
        /// List packages, versions, licences and what the lock pins.
        #[arg(long)]
        list: bool,
        /// Fetch and check the sources (sha256 and signature) of the named
        /// packages, or all, without building.
        #[arg(long)]
        fetch: bool,
    },
    /// Hash a pack directory into `.cache/oath/store` (same layout as the guest).
    Store {
        /// Pack directory (`bin/`, `lib/`, …).
        #[arg(long)]
        from: PathBuf,
        /// Catalog slot (`pkg:<name>`).
        #[arg(long)]
        name: String,
        /// Also write `{hash}.tar` next to the tree (object-storage blob).
        #[arg(long)]
        tar: bool,
    },
    /// Copy every package a seed profile names, at its locked hash and with
    /// its licence, into a store origin tree (`pkg/<name>/<hash>.tar`).
    Publish {
        /// Destination directory.
        #[arg(long)]
        dest: PathBuf,
    },
    /// Query OSV for packed trees that have `components.json`; write
    /// `.cache/oath/store/adv/<name>/<hash>.json`. Guest never calls OSV.
    /// Then `cargo make publish` copies `adv/`.
    Advisories {
        /// Only this slot (`curl` or `pkg:curl`). Default: every pack in the cache.
        #[arg(long)]
        name: Option<String>,
    },
    /// Move an installed machine to a build output without wiping it:
    /// ship missing packs, apply the output's pins (one generation), replace
    /// /lib/oath, archive the old boot and write the new one to its ESP.
    /// Requires `--confirm`.
    Update {
        /// `home@host[:port]` of a machine `install` wrote (its host key is
        /// pinned in build/known_hosts).
        #[arg(long)]
        target: Option<String>,
        /// Rehearse in QEMU: `install --qemu` this older output, then update it.
        #[arg(long, value_name = "OUTPUT")]
        qemu: Option<String>,
        /// Required. Writes the machine's boot files.
        #[arg(long)]
        confirm: bool,
        /// Reboot into the new boot and check it.
        #[arg(long)]
        reboot: bool,
        /// The target's ESP node (default: partition 1 of its root disk).
        #[arg(long)]
        esp: Option<String>,
        /// Build output to update to (default: the newest one `check` passed).
        #[arg(long)]
        output: Option<String>,
        /// Update to an output that `cargo make check` has not passed.
        #[arg(long)]
        unchecked: bool,
    },
    /// Rotate last-5 boot slots and write kernel+initrd+oath-efi to an existing ESP.
    /// Does not format a disk.
    Esp {
        /// VFAT ESP node (`/dev/sda1`).
        #[arg(long)]
        esp: String,
        /// Required. Writes firmware.
        #[arg(long)]
        confirm: bool,
        /// Root device the BLS `oath.root=` option should name (`/dev/sda2`).
        #[arg(long, default_value = "/dev/sda2")]
        root: String,
        /// Build output to write (default: the newest one `check` passed).
        #[arg(long)]
        output: Option<String>,
        /// Write an output that `cargo make check` has not passed.
        #[arg(long)]
        unchecked: bool,
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
        Cmd::Build { profile } => pack_image(&root, &out, &profile)?,
        Cmd::Run { build } => {
            if build {
                pack_image(&root, &out, oath_core::DEFAULT_PROFILE)?;
            }
            let rc = qemu::run_interactive(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Up { build } => {
            if build {
                pack_image(&root, &out, oath_core::DEFAULT_PROFILE)?;
            }
            let rc = qemu::run_up(&root, &out)?;
            std::process::exit(rc);
        }
        Cmd::Start { build } => {
            if build {
                pack_image(&root, &out, oath_core::DEFAULT_PROFILE)?;
            }
            qemu::start(&root, &out)?;
        }
        Cmd::Stop => qemu::stop(&out)?,
        Cmd::Check { build, only, jobs, target, output, agent, model } => {
            if build {
                pack_image(&root, &out, oath_core::DEFAULT_PROFILE)?;
            }
            let rc = if agent {
                agent::run_agent(&root, &out, agent::Opts { only, jobs, target, output, model })?
            } else {
                check::run_check(&root, &out, check::Opts { only, jobs, target, output })?
            };
            std::process::exit(rc);
        }
        Cmd::Ssh { args } => {
            let rc = qemu::ssh(&out, &args)?;
            std::process::exit(rc);
        }
        Cmd::Install { target, disk, confirm, qemu, usb, hostname, overlay, unchecked, output } => {
            install::run_install(
                &root,
                &out,
                install::Opts {
                    target,
                    disk,
                    confirm,
                    qemu,
                    usb,
                    hostname,
                    overlay,
                    unchecked,
                    output,
                },
            )?;
        }
        Cmd::Bootstrap { jobs } => {
            let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
            pkgs::bootstrap(&root, jobs.unwrap_or(n / 2).max(1))?
        }
        Cmd::Pkgs { names, list, fetch } => {
            // `oath` is this tree (pkg:oath), not a pkgs/ package.
            let (oath, rest): (Vec<String>, Vec<String>) =
                names.into_iter().partition(|n| n == "oath");
            if list {
                pkgs::list(&root)?
            } else if fetch {
                pkgs::fetch_all(&root, &rest)?
            } else {
                if oath.is_empty() || !rest.is_empty() {
                    pkgs::build(&root, &rest)?
                }
                if !oath.is_empty() {
                    oath_pkg::build(&root)?;
                }
            }
        }
        Cmd::Store { from, name, tar } => pack::store_pack(&root, &name, &from, tar)?,
        Cmd::Publish { dest } => pack::publish_store(&root, &dest)?,
        Cmd::Advisories { name } => advisories::fill_advisories(&root, name.as_deref())?,
        Cmd::Update { target, qemu, confirm, reboot, esp, output, unchecked } => {
            update::run_update(
                &root,
                &out,
                update::Opts { target, qemu, output, unchecked, confirm, reboot, esp },
            )?;
        }
        Cmd::Esp { esp, confirm, root: root_dev, output, unchecked } => {
            install::update_esp(
                &out,
                install::EspOpts { esp, confirm, root_dev, output, unchecked },
            )?;
        }
    }
    Ok(())
}

fn pack_image(root: &std::path::Path, out: &std::path::Path, profile: &str) -> Result<()> {
    let tools = tools::load(root)?;
    pack::build(root, out, &tools, profile).map(|_| ())
}
