//! A pinned tarball against upstream's git tag (R54).
//!
//! A signature says who published a tarball, not that it holds what the
//! project's history holds: xz's backdoor came signed by its maintainer, in a
//! file its tarball had and its git tree did not. So a new pin is compared
//! with its tag, file by file, by git's blob ids, from a clone that holds only
//! the tag's trees (build/fetch/git/). The result stays in
//! pkgs/tags/<name>.txt, so a new pin's diff shows what its tarball adds.
//!
//! A source can also rest on the tag itself: `git`, `tag` and `key` in
//! pkg.toml check the tag's signature, which covers the tree by hash, and the
//! tarball must then be that tree exactly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use sha1::{Digest, Sha1};

use crate::audit::Entry;
use crate::pkgs::{sha256, PkgToml};

const PGP_BEGIN: &[u8] = b"-----BEGIN PGP SIGNATURE-----";
const SSH_BEGIN: &[u8] = b"-----BEGIN SSH SIGNATURE-----";

/// A tag as upstream's repository has it.
pub struct Tag {
    pub name: String,
    pub commit: String,
    pub tree: String,
    /// The tag object, when the tag is annotated.
    object: Option<Vec<u8>>,
    /// Each file of the tree: path to blob id.
    files: BTreeMap<String, String>,
    /// Files the tree keeps as symlinks, which a release may copy in full.
    symlinks: BTreeSet<String>,
    /// Submodules: their files are not in this tree.
    links: Vec<String>,
    /// The clone, whose blobs git fetches when asked.
    dir: PathBuf,
}

/// Where a package's pin should be in upstream's repository.
pub struct Pin {
    pub url: String,
    pub tag: String,
    /// The tree's directory the tarball is made from.
    pub tag_dir: Option<String>,
    /// The tarball's directory the tree is in.
    pub tar_dir: Option<String>,
}

/// A file in a tarball, by git's name for its contents and, when it has CRLF
/// line ends, for those contents with LF: git may store one and export the other.
struct Unpacked {
    id: String,
    lf: Option<String>,
}

/// A tarball set against a tag.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Comparison {
    pub same: usize,
    /// In both, with other contents.
    pub differs: Vec<String>,
    /// In the tarball alone: what no one's git history vouches for.
    pub adds: Vec<String>,
    /// In the tag alone (a release may leave tests out).
    pub only_in_tag: usize,
}

impl Tag {
    /// Clones the tag once, trees and no file contents, and reads it.
    pub fn fetch(root: &Path, url: &str, name: &str) -> Result<Tag> {
        let dir = clone_dir(root, url, name)?;
        if !dir.is_dir() {
            let part = dir.with_extension("part");
            fs::create_dir_all(dir.parent().context("parent")?)?;
            let _ = fs::remove_dir_all(&part);
            git(
                None,
                &[
                    "clone",
                    "--quiet",
                    "--bare",
                    "--depth",
                    "1",
                    "--filter=blob:none",
                    "--single-branch",
                    "--branch",
                    name,
                    url,
                    part.to_str().context("path")?,
                ],
            )
            .with_context(|| format!("clone {url} at {name}"))?;
            fs::rename(&part, &dir)?;
        }
        let at = |what: &str| -> Result<String> {
            let spec = format!("refs/tags/{name}^{{{what}}}");
            let out = git(Some(&dir), &["rev-parse", "--verify", &spec])?;
            Ok(String::from_utf8_lossy(&out).trim().to_string())
        };
        let (commit, tree) = (at("commit")?, at("tree")?);
        let tag_ref = format!("refs/tags/{name}");
        let kind = git(Some(&dir), &["cat-file", "-t", &tag_ref])?;
        let object = if kind.trim_ascii() == b"tag" {
            Some(git(Some(&dir), &["cat-file", "tag", &tag_ref])?)
        } else {
            None
        };
        let listing = git(Some(&dir), &["ls-tree", "-r", "-z", &tree])?;
        let (files, symlinks, links) = parse_tree(&listing)?;
        Ok(Tag { name: name.to_string(), commit, tree, object, files, symlinks, links, dir })
    }

    /// How the tag is signed: `pgp <issuer>`, `ssh`, or `no`.
    pub fn signed(&self) -> String {
        let Some(object) = &self.object else { return "no".to_string() };
        if let Some((_, sig)) = split_signature(object, PGP_BEGIN) {
            return match crate::openpgp::issuer(sig) {
                Some(who) => format!("pgp {who}"),
                None => "pgp".to_string(),
            };
        }
        if split_signature(object, SSH_BEGIN).is_some() {
            return "ssh".to_string();
        }
        "no".to_string()
    }

    /// Checks the tag's signature against `keys`, and that what it signs
    /// names this tag and this commit. Returns the signing key's fingerprint.
    pub fn verify(&self, keys: &[u8]) -> Result<String> {
        let Some(object) = &self.object else { bail!("tag {} is not annotated", self.name) };
        let Some((payload, sig)) = split_signature(object, PGP_BEGIN) else {
            bail!("tag {} has no OpenPGP signature", self.name);
        };
        let who = crate::openpgp::verify(keys, sig, payload)?;
        let (mut commit, mut tag) = (None, None);
        for line in payload.split(|b| *b == b'\n').take_while(|l| !l.is_empty()) {
            let line = String::from_utf8_lossy(line);
            if let Some(v) = line.strip_prefix("object ") {
                commit = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("tag ") {
                tag = Some(v.to_string());
            }
        }
        if tag.as_deref() != Some(self.name.as_str()) {
            bail!("the signature is over tag {tag:?}, not {}", self.name);
        }
        if commit.as_deref() != Some(self.commit.as_str()) {
            bail!("the signature is over commit {commit:?}, not {}", self.commit);
        }
        Ok(who)
    }
}

impl Tag {
    /// The blob a symlink in the tree finally leads to.
    fn follow(&self, path: &str) -> Option<String> {
        let mut at = path.to_string();
        for _ in 0..8 {
            if !self.symlinks.contains(&at) {
                return self.files.get(&at).cloned();
            }
            let id = self.files.get(&at)?;
            let to = git(Some(&self.dir), &["cat-file", "blob", id]).ok()?;
            at = link_target(&at, &String::from_utf8_lossy(&to))?;
        }
        None
    }
}

fn clone_dir(root: &Path, url: &str, name: &str) -> Result<PathBuf> {
    let rel = url.split_once("://").map_or(url, |(_, r)| r).trim_end_matches(".git");
    let rel = format!("{}/{name}.git", rel.trim_end_matches('/'));
    if rel.split('/').any(|c| c.is_empty() || c == "..") {
        bail!("bad tag {url} {name}");
    }
    Ok(root.join("build/fetch/git").join(rel))
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<Vec<u8>> {
    let mut c = Command::new("git");
    if let Some(d) = dir {
        c.arg("-C").arg(d);
    }
    let out = c.args(args).env("GIT_TERMINAL_PROMPT", "0").output().context("run git")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

/// `git ls-tree -r -z`: files by path, the symlinks among them, and the
/// submodules' paths.
#[allow(clippy::type_complexity)]
fn parse_tree(listing: &[u8]) -> Result<(BTreeMap<String, String>, BTreeSet<String>, Vec<String>)> {
    let (mut files, mut symlinks, mut links) = (BTreeMap::new(), BTreeSet::new(), Vec::new());
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let Some((meta, path)) = text.split_once('\t') else { bail!("ls-tree: {text}") };
        let mut f = meta.split(' ');
        let (mode, id) = (f.next().unwrap_or(""), f.nth(1).unwrap_or(""));
        match mode {
            "160000" => links.push(path.to_string()),
            _ => {
                if mode == "120000" {
                    symlinks.insert(path.to_string());
                }
                files.insert(path.to_string(), id.to_string());
            }
        }
    }
    Ok((files, symlinks, links))
}

/// Where a symlink at `path` leads, within the tree.
fn link_target(path: &str, to: &str) -> Option<String> {
    let mut at: Vec<&str> = path.split('/').collect();
    at.pop();
    for part in to.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                at.pop()?;
            }
            p => at.push(p),
        }
    }
    Some(at.join("/"))
}

/// A tag object's signed part and its signature, which git appends last.
fn split_signature<'a>(object: &'a [u8], begin: &[u8]) -> Option<(&'a [u8], &'a [u8])> {
    let at = object.windows(begin.len()).rposition(|w| w == begin)?;
    (at == 0 || object[at - 1] == b'\n').then(|| object.split_at(at))
}

/// Git's name for a file's contents.
fn blob_id(data: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", data.len()).as_bytes());
    h.update(data);
    h.finalize().iter().fold(String::with_capacity(40), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Unpacks `tarball` and sets its files against the tag's, `tar_dir` of the
/// one against `tag_dir` of the other.
pub fn compare(
    root: &Path,
    tarball: &Path,
    tag: &Tag,
    tar_dir: Option<&str>,
    tag_dir: Option<&str>,
) -> Result<Comparison> {
    let build = root.join("build");
    fs::create_dir_all(&build)?;
    let tmp = tempfile::Builder::new().prefix("tag-compare-").tempdir_in(&build)?;
    crate::util::run(
        Command::new("tar").arg("-xf").arg(tarball).arg("-C").arg(tmp.path()).arg("--no-same-owner"),
    )
    .with_context(|| format!("unpack {}", tarball.display()))?;
    crate::util::run(Command::new("chmod").arg("-R").arg("u+rwX").arg(tmp.path()))?;
    let mut top = tmp.path().to_path_buf();
    let entries: Vec<_> = fs::read_dir(&top)?.collect::<std::io::Result<_>>()?;
    if let [only] = entries.as_slice() {
        if only.file_type()?.is_dir() {
            top = only.path();
        }
    }
    let mut unpacked = BTreeMap::new();
    walk(&top, "", &mut unpacked)?;
    let mut c = set_against(&unpacked, tag, tar_dir, tag_dir);
    // A file the tree keeps as a symlink and the release copies in full is
    // the same file, if it holds what the link leads to.
    let mut differs = Vec::new();
    for path in std::mem::take(&mut c.differs) {
        let at = under(&path, tar_dir).map(|p| match tag_dir {
            Some(d) => format!("{d}/{p}"),
            None => p.to_string(),
        });
        let followed = at.and_then(|p| tag.follow(&p));
        let u = &unpacked[&path];
        if followed.is_some_and(|id| id == u.id || u.lf.as_ref() == Some(&id)) {
            c.same += 1;
        } else {
            differs.push(path);
        }
    }
    c.differs = differs;
    Ok(c)
}

fn unpacked(data: &[u8]) -> Unpacked {
    let lf = data.windows(2).any(|w| w == b"\r\n").then(|| {
        let mut out = Vec::with_capacity(data.len());
        for (i, b) in data.iter().enumerate() {
            if !(*b == b'\r' && data.get(i + 1) == Some(&b'\n')) {
                out.push(*b);
            }
        }
        blob_id(&out)
    });
    Unpacked { id: blob_id(data), lf }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Unpacked>) -> Result<()> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name();
        let path = format!("{prefix}{}", String::from_utf8_lossy(name.as_bytes()));
        let kind = e.file_type()?;
        if kind.is_dir() {
            walk(&e.path(), &format!("{path}/"), out)?;
        } else if kind.is_symlink() {
            let to = fs::read_link(e.path())?;
            out.insert(path, Unpacked { id: blob_id(to.as_os_str().as_bytes()), lf: None });
        } else if kind.is_file() {
            out.insert(path, unpacked(&fs::read(e.path())?));
        }
    }
    Ok(())
}

fn under<'a>(path: &'a str, dir: Option<&str>) -> Option<&'a str> {
    match dir {
        Some(d) => path.strip_prefix(d)?.strip_prefix('/'),
        None => Some(path),
    }
}

fn set_against(
    unpacked: &BTreeMap<String, Unpacked>,
    tag: &Tag,
    tar_dir: Option<&str>,
    tag_dir: Option<&str>,
) -> Comparison {
    let files: BTreeMap<&str, &String> =
        tag.files.iter().filter_map(|(p, id)| Some((under(p, tag_dir)?, id))).collect();
    let mut c = Comparison::default();
    let mut seen = 0;
    for (path, u) in unpacked {
        match under(path, tar_dir).and_then(|p| files.get(p)) {
            Some(want) => {
                seen += 1;
                if **want == u.id || u.lf.as_ref() == Some(*want) {
                    c.same += 1;
                } else {
                    c.differs.push(path.clone());
                }
            }
            None => c.adds.push(path.clone()),
        }
    }
    c.only_in_tag = files.len() - seen;
    c
}

/// A source that rests on its tag: the signature checks out, and the tarball
/// is the signed tree. Returns the signing key's fingerprint and the tag.
pub fn check_signed_tag(
    root: &Path,
    url: &str,
    name: &str,
    keys: &[u8],
    tarball: &Path,
) -> Result<(String, Comparison)> {
    let tag = Tag::fetch(root, url, name)?;
    let who = tag.verify(keys)?;
    let c = compare(root, tarball, &tag, None, None)?;
    if !c.differs.is_empty() || !c.adds.is_empty() {
        bail!(
            "{} is not tag {name}'s tree: {} differ ({}), {} added ({})",
            tarball.display(),
            c.differs.len(),
            first(&c.differs),
            c.adds.len(),
            first(&c.adds)
        );
    }
    Ok((who, c))
}

fn first(paths: &[String]) -> String {
    paths.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
}

fn record_path(root: &Path, name: &str) -> PathBuf {
    crate::pkgs::pkgs_dir(root).join("tags").join(format!("{name}.txt"))
}

/// The tag a package's pin should be, from its audit entry.
pub fn tag_of(entry: &Entry, version: &str) -> Option<Pin> {
    if entry.no_compare.is_some() {
        return None;
    }
    let (repo, pattern) = (entry.repo.as_ref()?, entry.tag.as_ref()?);
    let v = crate::audit::pin_version(entry, version);
    Some(Pin {
        url: repo.clone(),
        tag: pattern.replace("{version_}", &v.replace('.', "_")).replace("{version}", &v),
        tag_dir: entry.tag_dir.clone(),
        tar_dir: entry.tar_dir.clone(),
    })
}

/// What a package's record says: the pin it was compared at, and its lines.
pub fn read_record(root: &Path, name: &str) -> Option<(String, Vec<String>)> {
    let text = fs::read_to_string(record_path(root, name)).ok()?;
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let sha = lines.iter().find_map(|l| l.strip_prefix("source "))?.rsplit(' ').next()?.to_string();
    Some((sha, lines))
}

/// Compares a package's pin with its tag unless its record already has this
/// pin, writes the record, and says what the pin adds that the last did not.
pub fn ensure(root: &Path, name: &str, pkg: &PkgToml, entry: &Entry) -> Result<()> {
    let Some(source) = pkg.source.first() else { return Ok(()) };
    let Some(pin) = tag_of(entry, &pkg.version) else { return Ok(()) };
    let before = read_record(root, name);
    if before.as_ref().is_some_and(|(sha, _)| *sha == source.sha256) {
        return Ok(());
    }
    let tarball = root.join("build/fetch").join(source.file());
    let tag = Tag::fetch(root, &pin.url, &pin.tag)?;
    let c = compare(root, &tarball, &tag, pin.tar_dir.as_deref(), pin.tag_dir.as_deref())?;
    let old: Vec<&str> = before
        .as_ref()
        .map(|(_, l)| l.iter().filter_map(|l| l.strip_prefix("adds ")).collect())
        .unwrap_or_default();
    let new: Vec<&String> = c.adds.iter().filter(|a| !old.contains(&a.as_str())).collect();
    let path = record_path(root, name);
    fs::create_dir_all(path.parent().context("parent")?)?;
    fs::write(&path, render(&source.file(), &sha256(&tarball)?, &tag, &c))?;
    eprintln!(
        ">> tag {} ({}): {} files the same, {} differ, {} only in the tarball, {} only in the tag",
        pin.tag,
        tag.signed(),
        c.same,
        c.differs.len(),
        c.adds.len(),
        c.only_in_tag
    );
    for d in &c.differs {
        eprintln!(">>   differs: {d}");
    }
    if !c.differs.is_empty() && entry.differs_because.is_none() {
        eprintln!(">>   read these, and say why in audit.toml's differs_because");
    }
    if before.is_some() {
        for a in &new {
            eprintln!(">>   new in the tarball alone: {a}");
        }
    }
    Ok(())
}

fn render(file: &str, sha: &str, tag: &Tag, c: &Comparison) -> String {
    let mut s = String::from(
        "# Written by `cargo make pkgs --fetch` (R54): this pin against upstream's tag.\n",
    );
    let _ = writeln!(s, "source {file} {sha}");
    let _ = writeln!(s, "tag {} commit {} tree {}", tag.name, tag.commit, tag.tree);
    let _ = writeln!(s, "signed {}", tag.signed());
    let _ = writeln!(s, "same {}", c.same);
    let _ = writeln!(s, "only-in-tag {}", c.only_in_tag);
    for l in &tag.links {
        let _ = writeln!(s, "submodule {l}");
    }
    for d in &c.differs {
        let _ = writeln!(s, "differs {d}");
    }
    for a in &c.adds {
        let _ = writeln!(s, "adds {a}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(files: &[(&str, &str)]) -> Tag {
        Tag {
            name: "v1".to_string(),
            commit: "c0".to_string(),
            tree: "t0".to_string(),
            object: None,
            files: files.iter().map(|(p, i)| (p.to_string(), i.to_string())).collect(),
            symlinks: BTreeSet::new(),
            links: Vec::new(),
            dir: PathBuf::new(),
        }
    }

    #[test]
    fn blob_ids_are_gits() {
        // printf 'hello\n' | git hash-object --stdin
        assert_eq!(blob_id(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn a_tree_listing_parses() {
        let l = b"100644 blob aa\tsrc/a.c\0100755 blob bb\trun me\0120000 blob dd\tCOPYING\0\
                  160000 commit cc\tthird/x\0";
        let (files, symlinks, links) = parse_tree(l).unwrap();
        assert_eq!(files["src/a.c"], "aa");
        assert_eq!(files["run me"], "bb");
        assert_eq!(files["COPYING"], "dd");
        assert!(symlinks.contains("COPYING"));
        assert_eq!(links, ["third/x"]);
    }

    #[test]
    fn a_link_is_followed_within_the_tree() {
        assert_eq!(link_target("COPYING", "LICENSE").unwrap(), "LICENSE");
        assert_eq!(link_target("a/b/ans.py", "../../ans.py").unwrap(), "ans.py");
        assert_eq!(link_target("icu4c/LICENSE", "../LICENSE").unwrap(), "LICENSE");
        assert!(link_target("LICENSE", "../../etc/passwd").is_none());
    }

    #[test]
    fn a_tarball_is_set_against_the_tag() {
        let files = |l: &[(&str, &str)]| -> BTreeMap<String, Unpacked> {
            l.iter()
                .map(|(p, i)| (p.to_string(), Unpacked { id: i.to_string(), lf: None }))
                .collect()
        };
        let u = files(&[("a", "1"), ("b", "2"), ("m4/x.m4", "3")]);
        let c = set_against(&u, &tag(&[("a", "1"), ("b", "9"), ("tests/t", "4")]), None, None);
        assert_eq!(c.same, 1);
        assert_eq!(c.differs, ["b"]);
        assert_eq!(c.adds, ["m4/x.m4"]);
        assert_eq!(c.only_in_tag, 1);

        // A tree in a subdirectory on either side.
        let u = files(&[("a", "1"), ("extra", "5")]);
        let c = set_against(&u, &tag(&[("sub/a", "1"), ("other/z", "8")]), None, Some("sub"));
        assert_eq!((c.same, c.adds.as_slice(), c.only_in_tag), (1, ["extra".to_string()].as_slice(), 0));
        let u = files(&[("nss/a", "1"), ("nspr/b", "2")]);
        let c = set_against(&u, &tag(&[("a", "1")]), Some("nss"), None);
        assert_eq!((c.same, c.adds.as_slice()), (1, ["nspr/b".to_string()].as_slice()));
    }

    #[test]
    fn crlf_the_tag_stores_as_lf_is_the_same_file() {
        let u: BTreeMap<String, Unpacked> =
            [("run.bat".to_string(), unpacked(b"echo\r\nhi\r\n"))].into();
        let c = set_against(&u, &tag(&[("run.bat", &blob_id(b"echo\nhi\n"))]), None, None);
        assert_eq!(c.same, 1);
    }

    #[test]
    fn the_signature_is_what_git_appended() {
        let object = b"object c0\ntype commit\ntag v1\n\nmsg\n-----BEGIN PGP SIGNATURE-----\n\nx\n";
        let (payload, sig) = split_signature(object, PGP_BEGIN).unwrap();
        assert!(payload.ends_with(b"msg\n"));
        assert!(sig.starts_with(PGP_BEGIN));
        // Text quoted in a tag's message, not at a line's start, is not it.
        assert!(split_signature(b"say -----BEGIN PGP SIGNATURE-----", PGP_BEGIN).is_none());
    }
}
