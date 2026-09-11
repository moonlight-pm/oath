**Date:** 2026-08-30
**Status:** target (freeze)
**Implementation:** not started (serving); identity locked. T32 hash-in-path is in.
**Dogfood:** none for a live origin (`pkg:fetchme` still QEMU wget canary)
**Gaps:** no guest store export; no peer discovery; signatures still out;
  bootstrap origin not deployed.
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Package hosting (Oath hosts as origin)

Extends [2026-08-30-wants-and-fetch.md](2026-08-30-wants-and-fetch.md).
Same verbs, same store, same `pkg.url`. Does not implement a new
fetch path in this freeze. Pack identity (content hash, hash-in-path
store, pin on `desired.hash`) is T32.

---

## Locks this freeze owns

- The **catalog object is the package.** Hosting is how `present=true`
  fills `/oath/store/pkg/<name>/`. No apt, no git-as-OS, no extra kind.
- **`pkg.url` is the v0 hosting primitive.** An origin is a URL on the
  object. Local store still wins if the tree is already there.
- **Another Oath host is a valid origin.** A machine that has a store
  tree may serve those bytes. A peer installs by setting `url` and
  applying. That is the “repository”: other Oath boxes, not a
  canonical archive and not a `repo` kind.
- **Git is this development repo**, and maybe later a place to publish
  catalog documents. It is not the live store. Apply does not clone,
  checkout, or speak refs.
- No new verbs. No package dependencies in this freeze.

T18’s “no package repository” means no apt/canonical archive. This
freeze is the identity of hosting, not a second index language.

**Amended 2026-09-11:** an HTTPS object (bucket, S3, static host) of
`{origin}/pkg/{name}/{hash}.tar` is a valid origin — that is still
`pkg.url`. The hash is of the pack tree, not the tar. A **bootstrap
default** origin is allowed so a new box can fill a missing pin; it is
not a `repo` kind and not canonical. Intended first deploy: an Oath
guest (or a static bucket of those tarballs) at
`https://store.oath.wicket.cloud`. Host development cache is
`.cache/oath/store` (`cargo make store --name <n> --from <dir> --tar`).

---

## Out

- Serving `/oath/store` from a guest (deferred)
- Content hash / signature / mirrors (hash identity is T32; signatures
  and mirrors still out)
- Discovering other Oath hosts
- Package deps, semver-style versions, a large binary archive
  (content-hash identity is T32)
- Git as apply or as the payload store
- Compositor / Sola / glibc runtime (T21)
