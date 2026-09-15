use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use oath_core::{build_file, file_plan, fmt_plan, hash_tree, seed, PkgNeed, PlanFile, KIND_PLAN};

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

fn pack_c_src(root: &Path) -> String {
    let stage = root.join("stage-hello-src");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(
        stage.join("hello.c"),
        "#include <stdio.h>\nint main(void) { puts(\"hello\"); return 0; }\n",
    )
    .unwrap();
    let h = hash_tree(&stage).unwrap();
    let dest = root.join("store/pkg/hello-src").join(&h);
    oath_core::copy_pack_tree(&stage, &dest).unwrap();
    h
}

fn pack_cc(root: &Path) -> String {
    let stage = root.join("stage-cc");
    let bin = stage.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    if let Some(cc) = which("cc").or_else(|| which("gcc")) {
        copy_exec(&cc, &bin.join("musl-cc"));
    } else {
        let body = "#!/bin/sh\n\
out=\n\
while [ $# -gt 0 ]; do\n\
  case \"$1\" in\n\
    -o) out=$2; shift 2 ;;\n\
    -*) shift ;;\n\
    *) shift ;;\n\
  esac\n\
done\n\
printf '#!/bin/sh\\nprintf hello\\\\n\\n' > \"$out\"\n\
chmod 0755 \"$out\"\n";
        std::fs::write(bin.join("musl-cc"), body).unwrap();
        let mut p = std::fs::metadata(bin.join("musl-cc")).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(bin.join("musl-cc"), p).unwrap();
    }
    let h = hash_tree(&stage).unwrap();
    let dest = root.join("store/pkg/cc").join(&h);
    oath_core::copy_pack_tree(&stage, &dest).unwrap();
    h
}

fn pack_busybox_old(root: &Path) -> String {
    let dest = root.join("store/pkg/busybox");
    let bin = dest.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let sh = which("sh").or_else(|| which("bash")).expect("sh");
    copy_exec(&sh, &bin.join("sh"));
    for t in ["mkdir", "cp", "chmod", "cat", "printf"] {
        if let Some(p) = which(t) {
            copy_exec(&p, &bin.join(t));
        }
    }
    hash_tree(&dest).unwrap()
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
    let obj = oath_core::Catalog::open(d.path())
        .unwrap()
        .get(&format!("{KIND_PLAN}:hello").parse().unwrap())
        .unwrap();
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
    assert!(err.contains("failed") || err.contains("unshare") || err.contains("sandbox"), "{err}");
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
    assert!(
        !d.path().join("store/plan/hello").exists() || {
            // file_plan not called
            std::fs::read_dir(d.path().join("store/plan/hello")).map(|rd| rd.count()).unwrap_or(0)
                == 0
        }
    );
}

#[test]
fn compile_with_declared_cc() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let cc = pack_cc(d.path());
    let src = pack_c_src(d.path());
    let src_hash = src.clone();
    let script = format!(
        "musl-cc -O2 -o /out/bin/hello /oath/store/pkg/hello-src/{src_hash}/hello.c\nchmod 0755 /out/bin/hello\n"
    );
    let p = PlanFile {
        name: "hello".into(),
        produces: "pkg:hello".into(),
        build_needs: vec![
            PkgNeed { id: "pkg:busybox".into(), hash: bb },
            PkgNeed { id: "pkg:cc".into(), hash: cc },
            PkgNeed { id: "pkg:hello-src".into(), hash: src },
        ],
        run_needs: vec![],
        script,
    };
    let s = fmt_plan(&p);
    let path = d.path().join("hello.plan");
    std::fs::write(&path, &s).unwrap();
    let r = build_file(d.path(), &path).expect("compile");
    assert!(!r.skipped);
    let product = d.path().join("store/pkg/hello").join(&r.product).join("bin/hello");
    assert!(product.is_file(), "{}", product.display());
    let out = std::process::Command::new(&product).output().expect("run hello");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(out.stdout, b"hello\n");
}

#[test]
fn old_layout_build_need() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox_old(d.path());
    let src = pack_src(d.path(), b"hello-old-layout\n");
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
    let r = build_file(d.path(), &path).expect("old-layout build");
    let product = d.path().join("store/pkg/hello").join(&r.product).join("bin/hello");
    assert_eq!(std::fs::read(&product).unwrap(), b"hello-old-layout\n");
}
