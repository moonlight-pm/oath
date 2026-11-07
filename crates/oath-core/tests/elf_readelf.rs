//! `elf::needed` against binutils' readelf on this host's /lib and /bin.
//! Ignored: run with `cargo test -p oath-core --test elf_readelf -- --ignored`.
use std::process::Command;

#[test]
#[ignore]
fn matches_readelf() {
    let mut checked = 0;
    for dir in ["/lib", "/bin"] {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let Ok(p) = std::fs::canonicalize(e.path()) else { continue };
            if !p.is_file() {
                continue;
            }
            let Some(ours) = oath_core::elf::needed(&p) else { continue };
            let out = Command::new("readelf").arg("-dW").arg(&p).output().unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            let theirs: Vec<String> = text
                .lines()
                .filter(|l| l.contains("(NEEDED)"))
                .filter_map(|l| Some(l.split('[').nth(1)?.trim_end_matches(']').to_string()))
                .collect();
            assert_eq!(ours, theirs, "{}", p.display());
            checked += 1;
        }
    }
    assert!(checked > 100, "{checked}");
    eprintln!("{checked} files agree");
}
