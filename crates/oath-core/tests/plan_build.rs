use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use oath_core::{
    build_file, file_plan, fmt_plan, hash_tree, seed, PlanFile, PkgNeed, KIND_PLAN,
};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p).find_map(|d| {
            let c = d.join(name);
            c.is_file().then_some(c)
        })
    })
}

fn copy_exec(src: &Path, dest: &Path) {
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::copy(src, dest).unwrap();
    let mut p = std::fs::metadata(dest).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(dest, p).unwrap();
}

fn pack_busybox(root: &Path) -> String {
    let stage = root.join("stage-busybox");
    let bin = stage.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let sh = which("sh").or_else(|| which("bash")).expect("sh");
    copy_exec(&sh, &bin.join("sh"));
    for t in ["mkdir", "cp", "chmod", "cat", "printf"] {
        if let Some(p) = which(t) {
            copy_exec(&p, &bin.join(t));
        }
    }
    let h = hash_tree(&stage).unwrap();
    let dest = root.join("store/pkg/busybox").join(&h);
    oath_core::copy_pack_tree(&stage, &dest).unwrap();
    h
}

fn pack_src(root: &Path, body: &[u8]) -> String {
    let stage = root.join("stage-src");
    let bin = stage.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("hello"), body).unwrap();
    let mut p = std::fs::metadata(bin.join("hello")).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(bin.join("hello"), p).unwrap();
    let h = hash_tree(&stage).unwrap();
    let dest = root.join("store/pkg/hello-src").join(&h);
    oath_core::copy_pack_tree(&stage, &dest).unwrap();
    h
}

#[test]
fn fmt_lint_file_plan() {
    let d = tmp();
    seed(d.path()).unwrap();
    let p = PlanFile {
        name: "hello".into(),
        produces: "pkg:hello".into(),
        build_needs: vec![],
        run_needs: vec![],
        script: "printf hi > /out/bin/hello\n".into(),
    };
    let s = fmt_plan(&p);
    let path = d.path().join("hello.plan");
    std::fs::write(&path, &s).unwrap();
    let hash = file_plan(d.path(), s.as_bytes()).unwrap();
    assert!(hash.starts_with("sha256-"));
    let stored = d.path().join("store/plan/hello").join(&hash).join("plan.plan");
    assert_eq!(std::fs::read(&stored).unwrap(), s.as_bytes());
    let obj = oath_core::Catalog::open(d.path()).unwrap().get(&format!("{KIND_PLAN}:hello").parse().unwrap()).unwrap();
    assert_eq!(obj.desired["hash"], hash);
}

#[test]
fn sandbox_relocate_and_cache() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let src = pack_src(d.path(), b"hello-from-src\n");
    let src_hash = src.clone();
    let script = format!(
        "mkdir -p /out/bin\ncp /oath/store/pkg/hello-src/{src_hash}/bin/hello /out/bin/hello\nchmod 0755 /out/bin/hello\n"
    );
    let p = PlanFile {
        name: "hello".into(),
        produces: "pkg:hello".into(),
        build_needs: vec![
            PkgNeed { id: "pkg:busybox".into(), hash: bb.clone() },
            PkgNeed { id: "pkg:hello-src".into(), hash: src },
        ],
        run_needs: vec![],
        script,
    };
    let s = fmt_plan(&p);
    let path = d.path().join("hello.plan");
    std::fs::write(&path, &s).unwrap();
    let r1 = build_file(d.path(), &path).expect("build");
    assert!(!r1.skipped);
    let product = d.path().join("store/pkg/hello").join(&r1.product).join("bin/hello");
    assert_eq!(std::fs::read(&product).unwrap(), b"hello-from-src\n");
    let r2 = build_file(d.path(), &path).expect("rebuild");
    assert_eq!(r2.product, r1.product);
}

#[test]
fn undeclared_cc_fails() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let p = PlanFile {
        name: "hello".into(),
        produces: "pkg:hello".into(),
        build_needs: vec![PkgNeed { id: "pkg:busybox".into(), hash: bb }],
        run_needs: vec![],
        script: "/bin/cc -o /out/bin/hello /tmp/x.c\n".into(),
    };
    let s = fmt_plan(&p);
    let path = d.path().join("hello.plan");
    std::fs::write(&path, &s).unwrap();
    let err = build_file(d.path(), &path).unwrap_err().to_string();
    assert!(
        err.contains("failed") || err.contains("unshare") || err.contains("sandbox"),
        "{err}"
    );
}

#[test]
fn adhoc_does_not_file_plan() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let src = pack_src(d.path(), b"x\n");
    let src_hash = src.clone();
    let script = format!(
        "mkdir -p /out/bin\ncp /oath/store/pkg/hello-src/{src_hash}/bin/hello /out/bin/hello\nchmod 0755 /out/bin/hello\n"
    );
    let p = PlanFile {
        name: "hello".into(),
        produces: "pkg:hello".into(),
        build_needs: vec![
            PkgNeed { id: "pkg:busybox".into(), hash: bb },
            PkgNeed { id: "pkg:hello-src".into(), hash: src },
        ],
        run_needs: vec![],
        script,
    };
    let s = fmt_plan(&p);
    let path = d.path().join("hello.plan");
    std::fs::write(&path, &s).unwrap();
    let _ = build_file(d.path(), &path);
    assert!(!d.path().join("store/plan/hello").exists() || {
        // file_plan not called
        std::fs::read_dir(d.path().join("store/plan/hello")).map(|rd| rd.count()).unwrap_or(0) == 0
    });
}
