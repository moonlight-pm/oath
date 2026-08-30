**Date:** 2026-08-30
**Status:** target (freeze)
**Implementation:** not started
**Dogfood:** none (identity only; `pkg:fetchme` already fetches a URL)
**Gaps:** no guest store export; no content hash/signature; no peer discovery
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Package hosting (Oath hosts as origin)

Extends [2026-08-30-wants-and-fetch.md](2026-08-30-wants-and-fetch.md).
Same verbs, same store, same `pkg.url`. Does not implement a new
fetch path in this freeze.

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

---

## Out

- Serving `/oath/store` from a guest (deferred)
- Content hash / signature / mirrors
- Discovering other Oath hosts
- Package deps, versions, a large binary archive
- Git as apply or as the payload store
- Compositor / Sola / glibc runtime
