use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use oath_core::{
    build_file, build_pinned, file_plan, fmt_plan, hash_tree, seed, write_json, Meta, PkgNeed,
    PlanFile, KIND_PLAN,
};
use serde_json::json;

static PATH_LOCK: Mutex<()> = Mutex::new(());

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

fn pack_upstream(root: &Path) -> (String, PathBuf) {
    let stage = root.join("stage-reloc-src");
    let payload = stage.join("hello-1.0");
    std::fs::create_dir_all(&payload).unwrap();
    let hello = payload.join("hello");
    std::fs::write(&hello, "#!/bin/sh\nprintf 'relocated\\n'\n").unwrap();
    let mut p = std::fs::metadata(&hello).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&hello, p).unwrap();
    let h = hash_tree(&stage).unwrap();
    let tar = root.join("reloc-src.tar");
    let st = std::process::Command::new("tar")
        .args(["-C", stage.to_str().unwrap(), "-cf", tar.to_str().unwrap(), "."])
        .status()
        .unwrap();
    assert!(st.success(), "tar pack reloc-src");
    (h, tar)
}

fn write_pkg_url(root: &Path, name: &str, url: &str, hash: &str) {
    let dir = root.join("objects/pkg").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    write_json(
        &dir.join("desired.json"),
        &json!({
            "present": false,
            "url": url,
            "hash": hash,
            "removable": true
        }),
    )
    .unwrap();
    write_json(
        &dir.join("actual.json"),
        &json!({
            "present": false,
            "links": [],
            "removable": true
        }),
    )
    .unwrap();
    write_json(&dir.join("meta.json"), &Meta::new("pkg", name, "mutate")).unwrap();
}

fn serve_file(path: &Path) -> (String, std::thread::JoinHandle<()>) {
    let bytes = std::fs::read(path).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = std::thread::spawn(move || {
        for _ in 0..8 {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf);
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-tar\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&bytes);
        }
    });
    (format!("http://{addr}/reloc-src.tar"), h)
}

fn with_wget_shim<T>(bin: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = PATH_LOCK.lock().unwrap();
    std::fs::create_dir_all(bin).unwrap();
    let wget = bin.join("wget");
    if which("wget").is_none() {
        let curl = which("curl").expect("curl to shim wget in host tests");
        std::fs::write(
            &wget,
            format!(
                "#!/bin/sh\n\
out=\n\
url=\n\
while [ $# -gt 0 ]; do\n\
  case \"$1\" in\n\
    -q) shift ;;\n\
    -O) out=$2; shift 2 ;;\n\
    -*) shift ;;\n\
    *) url=$1; shift ;;\n\
  esac\n\
done\n\
exec {} -sS -L --http1.1 -o \"$out\" \"$url\"\n",
                curl.display()
            ),
        )
        .unwrap();
        let mut p = std::fs::metadata(&wget).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&wget, p).unwrap();
    }
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old}", bin.display()));
    let r = f();
    std::env::set_var("PATH", old);
    r
}

fn reloc_plan(bb: &str, src: &str) -> PlanFile {
    let script = format!(
        "mkdir -p /out/bin\n\
cp /oath/store/pkg/reloc-src/{src}/hello-1.0/hello /out/bin/reloc\n\
chmod 0755 /out/bin/reloc\n"
    );
    PlanFile {
        name: "reloc".into(),
        produces: "pkg:reloc".into(),
        build_needs: vec![
            PkgNeed { id: "pkg:busybox".into(), hash: bb.to_string() },
            PkgNeed { id: "pkg:reloc-src".into(), hash: src.to_string() },
        ],
        run_needs: vec![],
        script,
    }
}

#[test]
fn prebuild_wget_relocate_file_and_skip() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let (src, tar) = pack_upstream(d.path());
    let (url, _srv) = serve_file(&tar);
    write_pkg_url(d.path(), "reloc-src", &url, &src);
    let p = reloc_plan(&bb, &src);
    let s = fmt_plan(&p);
    let path = d.path().join("reloc.plan");
    std::fs::write(&path, &s).unwrap();
    let r1 = with_wget_shim(&d.path().join("host-bin"), || {
        build_file(d.path(), &path).expect("pre-build wget + relocate")
    });
    assert!(!r1.skipped);
    let product = d.path().join("store/pkg/reloc").join(&r1.product).join("bin/reloc");
    assert!(product.is_file(), "{}", product.display());
    let out = std::process::Command::new(&product).output().expect("run reloc");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(out.stdout, b"relocated\n");
    assert!(!d.path().join("bin/reloc").exists());
    assert!(
        !d.path().join("store/plan/reloc").exists()
            || std::fs::read_dir(d.path().join("store/plan/reloc"))
                .map(|rd| rd.count())
                .unwrap_or(0)
                == 0
    );

    let hash = file_plan(d.path(), s.as_bytes()).unwrap();
    let r2 = build_pinned(d.path(), "reloc").expect("first pinned");
    assert_eq!(r2.product, r1.product);
    assert_eq!(r2.plan_hash, hash);
    let r3 = build_pinned(d.path(), "reloc").expect("second pinned");
    assert!(r3.skipped);
    assert_eq!(r3.product, r1.product);
    assert!(!d.path().join("bin/reloc").exists());
}

#[test]
fn prebuild_missing_url_refuses() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let fake = "sha256-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let p = reloc_plan(&bb, fake);
    let s = fmt_plan(&p);
    let path = d.path().join("reloc.plan");
    std::fs::write(&path, &s).unwrap();
    let err = build_file(d.path(), &path).unwrap_err().to_string();
    assert!(err.contains("not in the store") || err.contains("reloc-src"), "{err}");
}

#[test]
fn prebuild_wget_hash_mismatch_refuses() {
    let d = tmp();
    seed(d.path()).unwrap();
    let bb = pack_busybox(d.path());
    let (_src, tar) = pack_upstream(d.path());
    let (url, _srv) = serve_file(&tar);
    let wrong = "sha256-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    write_pkg_url(d.path(), "reloc-src", &url, wrong);
    let p = reloc_plan(&bb, wrong);
    let s = fmt_plan(&p);
    let path = d.path().join("reloc.plan");
    std::fs::write(&path, &s).unwrap();
    let err = with_wget_shim(&d.path().join("host-bin"), || {
        build_file(d.path(), &path).unwrap_err().to_string()
    });
    assert!(
        err.contains("hash") || err.contains("does not match") || err.contains("failed"),
        "{err}"
    );
}
