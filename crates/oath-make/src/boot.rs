//! ESP boot generations: BLS text, archive rotation, loader.conf.

use anyhow::{bail, Context, Result};
use oath_core::{boot_subvol_name, rotate_boot_ids, BOOT_ARCHIVES};

use std::fs;
use std::path::Path;

/// QEMU EFI rehearsal must not wait on a menu (`cargo make probe` is
/// still `-kernel`; this is for `install --qemu` only).
pub const LOADER_CONF_QEMU: &str = "default oath.conf\ntimeout 0\neditor no\nconsole-mode auto\n";

/// Metal: five seconds to pick an archived boot.
pub const LOADER_CONF_METAL: &str = "default oath.conf\ntimeout 5\neditor no\nconsole-mode auto\n";

pub struct BlsEntry {
    pub title: String,
    pub linux: String,
    pub initrd: String,
    pub options: String,
}

pub fn parse_loader_conf(text: &str) -> (String, u32) {
    let mut default = String::from("oath.conf");
    let mut timeout = 0u32;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("default ") {
            default = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("timeout ") {
            timeout = rest.trim().parse().unwrap_or(0);
        }
    }
    (default, timeout)
}

pub fn parse_bls(text: &str) -> BlsEntry {
    let mut e = BlsEntry {
        title: String::from("Oath"),
        linux: String::from("/vmlinuz"),
        initrd: String::from("/initrd.gz"),
        options: String::new(),
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("title ") {
            e.title = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("linux ") {
            e.linux = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("initrd ") {
            e.initrd = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("options ") {
            e.options = rest.trim().to_string();
        }
    }
    e
}

pub fn parse_oath_boots(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|s| s.to_string())
        .collect()
}

pub fn format_oath_boots(names: &[String]) -> String {
    let mut s = String::from("# newest first; oath-efi reads this\n");
    for n in names {
        s.push_str(n);
        s.push('\n');
    }
    s
}

pub fn current_bls(root_dev: &str, extra_options: &str) -> String {
    format!(
        "title Oath\nsort-key 9999\nlinux /vmlinuz\ninitrd /initrd.gz\noptions {opts}\n",
        opts = kernel_options(root_dev, "@", extra_options),
    )
}

pub fn archive_bls(id: u64, root_dev: &str, extra_options: &str) -> String {
    let sub = boot_subvol_name(id);
    format!(
        "title Oath boot {id}\nsort-key {id}\nlinux /oath/boot/{id}/vmlinuz\ninitrd /oath/boot/{id}/initrd.gz\noptions {opts}\n",
        opts = kernel_options(root_dev, &sub, extra_options),
    )
}

pub fn kernel_options(root_dev: &str, subvol: &str, extra: &str) -> String {
    let mut opts = format!(
        "console=ttyS0,115200 console=tty0 amdgpu.si_support=1 radeon.si_support=0 oath.root={root_dev} oath.subvol={subvol}"
    );
    for tok in extra.split_whitespace() {
        if tok.starts_with("oath.root=") || tok.starts_with("oath.subvol=") {
            continue;
        }
        opts.push(' ');
        opts.push_str(tok);
    }
    opts
}

pub fn archive_conf_name(id: u64) -> String {
    format!("oath-{id}.conf")
}

/// Existing archive ids from `loader/entries/oath-N.conf` names.
pub fn existing_archive_ids(names: &[String]) -> Vec<u64> {
    let mut ids = Vec::new();
    for n in names {
        let stem = n.strip_suffix(".conf").unwrap_or(n);
        if let Some(rest) = stem.strip_prefix("oath-") {
            if rest.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(id) = rest.parse::<u64>() {
                    ids.push(id);
                }
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub struct RotatePlan {
    pub new_id: u64,
    pub prune: Vec<u64>,
    pub boots: Vec<String>,
}

pub fn plan_rotate(existing_ids: &[u64]) -> RotatePlan {
    let (new_id, prune) = rotate_boot_ids(existing_ids, BOOT_ARCHIVES);
    let mut keep: Vec<u64> = existing_ids.iter().copied().filter(|i| !prune.contains(i)).collect();
    keep.push(new_id);
    keep.sort_unstable();
    keep.reverse();
    let mut boots = vec![String::from("oath.conf")];
    for id in &keep {
        boots.push(archive_conf_name(*id));
    }
    RotatePlan { new_id, prune, boots }
}

/// Scan an already-mounted ESP for `oath-N.conf` filenames.
pub fn list_archive_confs(entries_dir: &Path) -> Result<Vec<String>> {
    if !entries_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for e in fs::read_dir(entries_dir).with_context(|| format!("read {}", entries_dir.display()))? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("oath-") && name.ends_with(".conf") && name != "oath-install.conf" {
            names.push(name);
        }
    }
    Ok(names)
}

pub fn apply_rotate_files(
    esp: &Path,
    new_kernel: &Path,
    new_initrd: &Path,
    new_efi: Option<&Path>,
    root_dev: &str,
    extra_options: &str,
    loader_conf: &str,
) -> Result<(u64, Vec<u64>)> {
    let entries = esp.join("loader/entries");
    fs::create_dir_all(&entries)?;
    fs::create_dir_all(esp.join("oath/boot"))?;
    let names = list_archive_confs(&entries)?;
    let ids = existing_archive_ids(&names);
    let plan = plan_rotate(&ids);

    let cur_k = esp.join("vmlinuz");
    let cur_i = esp.join("initrd.gz");
    let mut archived = false;
    if cur_k.is_file() && cur_i.is_file() {
        let dest = esp.join("oath/boot").join(plan.new_id.to_string());
        fs::create_dir_all(&dest)?;
        fs::copy(&cur_k, dest.join("vmlinuz")).context("archive vmlinuz")?;
        fs::copy(&cur_i, dest.join("initrd.gz")).context("archive initrd")?;
        fs::write(
            entries.join(archive_conf_name(plan.new_id)),
            archive_bls(plan.new_id, root_dev, extra_options),
        )?;
        archived = true;
    }

    for id in &plan.prune {
        let _ = fs::remove_dir_all(esp.join("oath/boot").join(id.to_string()));
        let _ = fs::remove_file(entries.join(archive_conf_name(*id)));
    }

    let mut boots = vec![String::from("oath.conf")];
    let mut keep: Vec<u64> = ids.iter().copied().filter(|i| !plan.prune.contains(i)).collect();
    if archived {
        keep.push(plan.new_id);
    }
    keep.sort_unstable();
    keep.reverse();
    for id in keep {
        boots.push(archive_conf_name(id));
    }

    fs::copy(new_kernel, &cur_k).context("copy vmlinuz")?;
    fs::copy(new_initrd, &cur_i).context("copy initrd")?;
    if let Some(efi) = new_efi {
        let boot = esp.join("EFI/BOOT");
        fs::create_dir_all(&boot)?;
        fs::copy(efi, boot.join("BOOTX64.EFI")).context("copy BOOTX64")?;
    }
    fs::write(entries.join("oath.conf"), current_bls(root_dev, extra_options))?;
    fs::create_dir_all(esp.join("loader"))?;
    fs::write(esp.join("loader/loader.conf"), loader_conf)?;
    fs::write(esp.join("loader/oath-boots"), format_oath_boots(&boots))?;
    Ok((plan.new_id, plan.prune))
}

pub fn require_confirm(confirm: bool) -> Result<()> {
    if !confirm {
        bail!("refusing to write the ESP without --confirm");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_timeout() {
        let (d, t) = parse_loader_conf(LOADER_CONF_METAL);
        assert_eq!(d, "oath.conf");
        assert_eq!(t, 5);
        let (_, z) = parse_loader_conf(LOADER_CONF_QEMU);
        assert_eq!(z, 0);
    }

    #[test]
    fn bls_roundtrip_current() {
        let t = current_bls("/dev/sda2", "");
        let e = parse_bls(&t);
        assert_eq!(e.linux, "/vmlinuz");
        assert!(e.options.contains("oath.subvol=@"));
        assert!(e.options.contains("oath.root=/dev/sda2"));
    }

    #[test]
    fn archive_points_at_slot() {
        let t = archive_bls(3, "/dev/sda2", "");
        let e = parse_bls(&t);
        assert_eq!(e.linux, "/oath/boot/3/vmlinuz");
        assert!(e.options.contains("oath.subvol=@boot-3"));
    }

    #[test]
    fn rotate_index_newest_first() {
        let p = plan_rotate(&[1, 2]);
        assert_eq!(p.new_id, 3);
        assert_eq!(p.boots[0], "oath.conf");
        assert!(p.boots.contains(&"oath-3.conf".into()));
    }

    #[test]
    fn existing_ids_from_names() {
        let ids = existing_archive_ids(&[
            "oath-2.conf".into(),
            "oath.conf".into(),
            "oath-install.conf".into(),
            "oath-10.conf".into(),
        ]);
        assert_eq!(ids, vec![2, 10]);
    }

    #[test]
    fn rotate_writes_esp() {
        let tmp = std::env::temp_dir().join(format!("oath-esp-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let k1 = tmp.join("k1");
        let i1 = tmp.join("i1");
        fs::write(&k1, b"oldk").unwrap();
        fs::write(&i1, b"oldi").unwrap();
        fs::copy(&k1, tmp.join("vmlinuz")).unwrap();
        fs::copy(&i1, tmp.join("initrd.gz")).unwrap();
        let k2 = tmp.join("k2");
        let i2 = tmp.join("i2");
        fs::write(&k2, b"newk").unwrap();
        fs::write(&i2, b"newi").unwrap();
        let (id, prune) = apply_rotate_files(&tmp, &k2, &i2, None, "/dev/sda2", "", LOADER_CONF_METAL).unwrap();
        assert_eq!(id, 1);
        assert!(prune.is_empty());
        assert_eq!(fs::read(tmp.join("vmlinuz")).unwrap(), b"newk");
        assert_eq!(fs::read(tmp.join("oath/boot/1/vmlinuz")).unwrap(), b"oldk");
        let boots = parse_oath_boots(&fs::read_to_string(tmp.join("loader/oath-boots")).unwrap());
        assert_eq!(boots[0], "oath.conf");
        let _ = fs::remove_dir_all(&tmp);
    }
}
