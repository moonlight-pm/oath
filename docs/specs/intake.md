# Intake: what enters Oath as someone else's code

Decision: R57. **Is** means it runs today; **will be** means designed, not built.

## The order of preference

Before a package, in this order:

1. What Oath already ships.
2. Our own small code, kept in this repo (`apps/`), when writing it is not too
   expensive. Not TLS, not a database, not a compiler.
3. A compiled project with a signed release and few dependencies.
4. A package with a large dependency tree, last.

Whoever adds one says what the cheaper route was and why it lost, and names its
dependency count. A tree nobody can read is a cost, and it counts against a package
as its bugs would. Dropping one is worth doing whenever a small replacement exists:
Podman and its netavark became 1,800 lines of our own Go, which took 157 modules,
257 crates, conmon, protobuf and Abseil with them.

## The gates (is)

They apply to the Go, npm, Python and Rust packages a pin brings. Each package's
are recorded in `pkgs/intake/<name>.txt`, `pkg:oath`'s from `Cargo.lock`.

| Gate | Where | What it does |
|------|-------|--------------|
| Age | `cargo make pkgs --fetch` | A version that moved or is new must be 7 days old. A release too, by its tag's commit or its server's date. |
| Lowest fixed | by rule | A fix takes the lowest version that has it and is a week old, not the newest. |
| Exception | `age_because` in `pkgs/audit.toml` | A fix younger than that names its advisory, for that one version. |
| New names | `pkgs/intake/approved.txt` | A new dependency, or an npm package whose publisher changed, stops until a person writes it here. |
| Review | `pkgs/reviews.txt`, and a test | Every version that moved or is new has a verdict. `cargo make pkgs --review <name>` writes the diff, and what in it looks like new reach. |
| Stop the line | `cargo make audit`, `cargo make publish` | An OSV `MAL-` record fails the audit and refuses a release. Nothing waves it through. |
| Tag | `pkgs/tags/<name>.txt` (R54) | The tarball against upstream's git tree. |
| Defaults | `pkg:node`'s npm | `min-release-age=7`, `ignore-scripts=true`. Python ships no `pip`. `pkg:bun`'s `bin/bun` puts `--ignore-scripts --minimum-release-age=604800` in `BUN_OPTIONS`, above a project's settings (R85). |

What was already here on 2026-11-01 is the baseline: recorded, not reviewed. Nobody
reads 537 packages at once. Gates apply to what moves after that, and nothing else
becomes baseline: a new package's modules are new names and its release must be a week
old, and deleting a record does not change that. The one exception is
`pkg:cloudflared`'s 67 modules, which Joshua accepted as a baseline (2026-11-03).

### Who decides

- The unattended audit may approve a bump of an existing dependency, once its
  review is written and passes. It may name an advisory for a young fix.
- A new name or publisher waits for Joshua. The audit may not write `approved.txt`.
- The audit may pin a new signing key that upstream's own site or repository names (R67).
  A signer upstream does not name stops the pin.

### Reviewing

The diff is data. It can try to instruct whoever reads it, so the reader has no
tools but reading, and treats nothing in it as a request. A verdict is one line:

```
go golang.org/x/net v0.63.0 2026-11-20 audit: HTTP/2 flow control and two tests; no new reach
```

The flags are heuristics that say where to look. Finding none says nothing. The
reader looks for what a security fix or a version bump would not need: a process,
the network, an install hook, secrets, a blob, a change unrelated to the notes.

## What this does not stop

- A patient maintainer. xz sat live for five weeks, event-stream for ten, and both
  passed any cooldown. Review improves the odds; nothing guarantees it.
- Signed provenance is not safety: TanStack's malicious versions carried valid SLSA
  attestations from its real pipeline. A signature says who, never what.
- Go's dates are commits' dates, which an author can set: the weakest of the ages.
- Files only in a tarball that no history vouches for (`configure`, `Makefile.in`).

So containment matters as much as intake: a server runs as `work` (R55), with no
`sudo` and `/home` closed.

## Will be

- **Project installs in a sandbox.** Coding agents run as `home`, which has `sudo`
  with no password, so a poisoned install in a project reaches everything. npm's
  scripts are off by default; `cargo build` runs `build.rs` and the program itself
  always runs. The sandbox (`bwrap`: the project and its caches, no `/home`) is next.
- **Age limits for `go` and `cargo` in a project.** Neither has a stable setting yet;
  pins in `pkgs/` have the gate regardless.
- **Shared audits.** Importing Mozilla's and Google's `cargo-vet` records for crates.
- **Fewer trees.** See [dependencies.md](dependencies.md).
