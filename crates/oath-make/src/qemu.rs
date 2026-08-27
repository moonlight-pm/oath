use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::tools::Tools;
use crate::util::{kvm, sha256_file, utc_rfc3339, utc_stamp, write_pretty};

pub struct Image {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub backing: PathBuf,
}

pub fn load_image(out: &Path) -> Result<Image> {
    let kernel =
        std::env::var_os("OATH_KERNEL").map(PathBuf::from).unwrap_or_else(|| out.join("bzImage"));
    let backing =
        std::env::var_os("OATH_IMAGE").map(PathBuf::from).unwrap_or_else(|| out.join("oath.qcow2"));
    let initrd = out.join("initrd.gz");
    for p in [&kernel, &backing, &initrd] {
        if !p.is_file() {
            bail!("missing {} — run: cargo run -p oath-make -- build", p.display());
        }
    }
    Ok(Image { kernel, initrd, backing })
}

pub fn new_run(out: &Path, label: &str) -> Result<PathBuf> {
    let dir = out.join("runs").join(format!("{}-{label}", utc_stamp()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn overlay_disk(tools: &Tools, run: &Path, backing: &Path) -> Result<PathBuf> {
    let overlay = run.join("disk.qcow2");
    if !overlay.is_file() {
        let backing = fs::canonicalize(backing)?;
        crate::util::run(Command::new(&tools.qemu_img).args([
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            backing.to_str().unwrap(),
            overlay.to_str().unwrap(),
        ]))?;
    }
    Ok(overlay)
}

pub fn write_meta(
    run: &Path,
    tools: &Tools,
    img: &Image,
    overlay: &Path,
    kind: &str,
) -> Result<()> {
    write_pretty(
        &run.join("meta.json"),
        &json!({
            "started": utc_rfc3339(),
            "kind": kind,
            "qemu": tools.qemu.display().to_string(),
            "kvm": kvm(),
            "kernel": img.kernel.display().to_string(),
            "initrd": img.initrd.display().to_string(),
            "backing": img.backing.display().to_string(),
            "overlay": overlay.display().to_string(),
            "sha256": {
                "kernel": sha256_file(&img.kernel).ok(),
                "initrd": sha256_file(&img.initrd).ok(),
                "backing": sha256_file(&img.backing).ok(),
            }
        }),
    )
}

pub fn qemu_args(
    tools: &Tools,
    img: &Image,
    overlay: &Path,
    serial_log: &Path,
    qemu_log: &Path,
) -> Vec<String> {
    let mut a = vec![tools.qemu.display().to_string(), "-machine".into(), "q35".into()];
    if kvm() {
        a.push("-enable-kvm".into());
    }
    a.extend([
        "-m".into(),
        "512".into(),
        "-display".into(),
        "none".into(),
        "-monitor".into(),
        "none".into(),
        "-chardev".into(),
        format!("stdio,id=cons,logfile={},signal=off", serial_log.display()),
        "-serial".into(),
        "chardev:cons".into(),
        "-kernel".into(),
        img.kernel.display().to_string(),
        "-initrd".into(),
        img.initrd.display().to_string(),
        "-append".into(),
        "console=ttyS0 panic=10".into(),
        "-drive".into(),
        format!("file={},if=virtio,format=qcow2,cache=writeback", overlay.display()),
        "-d".into(),
        "guest_errors".into(),
        "-D".into(),
        qemu_log.display().to_string(),
        "-no-reboot".into(),
    ]);
    a
}

pub fn run_interactive(root: &Path, out: &Path) -> Result<i32> {
    let tools = crate::tools::load(root)?;
    let img = load_image(out)?;
    let run = new_run(out, "int")?;
    let overlay = overlay_disk(&tools, &run, &img.backing)?;
    write_meta(&run, &tools, &img, &overlay, "int")?;
    let args = qemu_args(&tools, &img, &overlay, &run.join("serial.log"), &run.join("qemu.log"));
    fs::write(run.join("qemu.cmd"), args.join(" ") + "\n")?;
    eprintln!("run: {}", run.display());
    eprintln!("serial log: {}", run.join("serial.log").display());
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd.status().context("qemu")?;
    let rc = status.code().unwrap_or(1);
    if let Ok(mut meta) =
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(run.join("meta.json"))?)
    {
        meta["ended"] = json!(utc_rfc3339());
        meta["qemu_exit"] = json!(rc);
        write_pretty(&run.join("meta.json"), &meta)?;
    }
    eprintln!("qemu exit {rc}  (logs in {})", run.display());
    Ok(rc)
}
