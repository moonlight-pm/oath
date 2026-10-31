---
name: oath-audit
description: The daily security audit of every Oath package (R48) — scan, triage, fix, check, publish, report. Use when the scheduled run starts it (svc:audit → check/audit/run, at 04:00 on buccina), or when Joshua asks for a security audit, a CVE check, or "are we vulnerable to …".
---

# Oath security audit (R48)

`svc:audit` runs `check/audit/run` on buccina at 04:00 Mountain as
`home`: one headless run of this skill with the permissions in
`check/audit/settings.json` (anything not listed is refused; nobody is
there to ask). Joshua's standing word (2026-10-31): fix what is found,
publish the fix, and buccina takes it; the runner mails him your report
and then updates and reboots buccina. novus and canto are never yours.

## 1. Scan

```
cargo make audit
```

It reads `pkgs/audit.toml` and matches every package against cve.org's
CVE List V5 (its local mirror updates first), OSV, NVD (unkeyed, about
20 minutes), CISA KEV and EPSS, and writes `build/audit/latest.json`.
A source that failed says so in `sources`: the report says it too, and
the run goes on with the others. A package with no entry in
`pkgs/audit.toml` stops the scan: add its entry (its NVD CPE, the names
CVE records use, its repo and tag, or `skip` and why).

## 2. Triage, most urgent first

Order: known exploited (`kev`), then `affected`, then EPSS. For each hit,
check it against upstream before believing it: the CVE record's
references, the project's security page or release notes (WebFetch), and
`pkgs/<name>/build.sh` for what Oath builds and turns off. Then one of:

- **Not affected**: another product with the name, a platform Oath does
  not build (Windows, macOS, Android), a feature or backend
  `build.sh` turns off, or a version range the scan misread. Add
  `"CVE-…" = "why, and the source that says so"` under
  `[<name>.not_affected]` in `pkgs/audit.toml`.
- **A fixed upstream release exists**: upgrade (step 3).
- **Upstream fixed it in a commit but has not released it**: apply
  upstream's own patch as `pkgs/<name>/patches/upstream-CVE-….patch`,
  its first lines naming the upstream commit URL.
- **No upstream fix yet, and it matters** (known exploited, high or
  critical, or EPSS ≥ 0.1, and Oath builds the affected code): write a
  backport as `pkgs/<name>/patches/oath-backport-CVE-….patch`. Its
  header says, in this order: `Oath backport, not upstream's.`, the CVE,
  what it fixes and how, and `Replace with upstream's fix when it lands.`
  Otherwise report it and leave it.
- **`unknown`** (a record names the product with no versions): decide
  from upstream if you can; else it goes under "Waiting on Joshua".

Every run, for each `oath-backport-*.patch` in `pkgs/*/patches/`: has
upstream fixed that CVE since (a release, or a commit)? Then replace the
backport with the release or upstream's patch, and say so.

## 3. Fix

- **Upgrade**: in `pkgs/<name>/pkg.toml`, the newest stable upstream
  release that fixes it (no release candidates): `version`, and each
  `[[source]]`'s `url`, `sha256` (of the file you fetched) and `sig`.
  The signature must verify with the key `key` names in `pkgs/keys/`;
  a release signed by a key not there is Joshua's to trust (R37): leave
  that package as it is and report it. The kernel moves to the newest
  stable `7.3.y` (kernel.org's tarballs, signed by Greg Kroah-Hartman,
  `pkgs/keys/linux.asc`).
- **Patches** go in `pkgs/<name>/patches/`; check `build.sh` applies them
  (most apply every `patches/*.patch` in order; if not, add the `patch`
  line there).
- **Recipes** (claude-code, codex, grok-build, nvidia, steam) are vendor
  binaries checked against the vendor's own signature: upgrade as their
  `pkg.toml` does it. **Packs from elsewhere** (Sola's) are not Oath's:
  report them; never edit their pins in `crates/oath-core/seed/`.
- Then `cargo make pkgs -j 8`: it rebuilds each package whose plan
  moved, the ones that build with it included, and updates
  `pkgs/lock.json`. A fix that would rebuild the toolchain (gcc, glibc,
  binutils, rust) or CEF (about 4.5 hours) is still a fix: do it if it is
  known exploited or critical; otherwise report it under "Waiting on
  Joshua" with what it would rebuild.
- `cargo test -p oath-make audit` if you edited `pkgs/audit.toml`.

## 4. Check and release

Only when something changed:

```
cargo make build --profile desk
cargo make check
```

`cargo make check` must pass every case (it prints `N pass, 0 fail`).
A failure: undo that package's change (`git restore` before a commit,
`git revert` after), build and check again without it, and report what
failed. Then:

- One commit per package: `Security: pkg:<name> <old> -> <new> (CVE-…,
  CVE-…)`, or `… (oath backport of CVE-…)`, with what you verified in the
  body and the `Co-Authored-By` line this session's instructions give.
- `git push origin master`, then `cargo make publish`. It refuses unless
  the tree is clean and equals `origin/master` and the output was checked:
  never work around it. Write the output id it published, alone, to the
  file the prompt names.

## 5. Report

Write the report where the prompt says, as plain Markdown. Its first
line is the mail's subject: `# Oath audit <day>: <what happened>` (for
example `2 fixed and published, 1 waits on you` or `nothing affected`).
Then, each section only if it has something:

- **Fixed and published**: package, old -> new, the CVEs, how (upstream
  release, upstream patch, Oath backport), the output id.
- **Backports in place**: each, and whether upstream has fixed it yet.
- **Waiting on Joshua**: new signing keys, big rebuilds, unknowns that
  need judgment, a failed check, a source that was down.
- **Not affected**: what you added to `pkgs/audit.toml` and why.
- **Seen**: the counts the scan reported, by package.

## Never

`sudo`, `ssh`, `oath` on any machine, `cargo make install` or `update`,
a force push or a reset; an edit to `pkgs/keys/`, `crates/`, `check/` or
`.claude/`; a publish of anything `cargo make check` did not pass; a
backport not marked as one.
