**Date:** 2026-09-16
**Status:** target (freeze)
**Implementation:** partial (schema + actual fields + unit tests +
  pack-script `components.json` for cc/rustc/bash/cmake/pkg-config;
  sola-oath shows components and advisories; `cargo make advisories`
  queries OSV into the host store `adv/` tree)
**Dogfood:** host unit tests (including local OSV + wget fetch). Canto
  live trees are old-layout and have no `components.json` until the
  next pack. Origin `adv/` empty until `advisories` + `publish`.
**Gaps:** guest `oath` / sola-oath live-copied until next pack; most
  relocate/nix packs still omit `components.json`; origin `adv/` not
  published yet; extra on-disk realizations do not fetch their own
  advisories; no NVD client (OSV only)
**As-built:** [../capabilities.md](../capabilities.md) · [../architecture.md](../architecture.md)

# Package upstream identity and hash-keyed advisories

Amends [2026-09-03-pkg-pack-identity.md](2026-09-03-pkg-pack-identity.md)
(hash is still the pin; a version string is not identity),
[2026-09-13-pkg-about.md](2026-09-13-pkg-about.md) (`description` /
`home` stay slot prose; they may lag a new pack),
[2026-08-30-pkg-hosting.md](2026-08-30-pkg-hosting.md) (T20 origin
gains an `adv/` tree; still not a `repo` kind). Same verbs. No new
kind.

Public feeds (OSV, NVD, GHSA) speak **upstream name + version**.
Oath speaks **content hash**. Both are required: components are how
you match a feed; advisories keyed by hash are how this distro
asserts that **these bits** are a known risk (relocated, patched, or
a blob of several upstreams).

---

## Locks this freeze owns

- **No `version` field on the slot.** `pkg:curl` is a name. Two hashes
  of that name can be different programs (T32). A single catalog
  `version` would lie when the pin moves.
- **`components.json` is pack-tree fact.** File at the pack root
  (next to optional `INDEX.md`). It is part of `oath-tree-v1`. Changing
  the claim changes the hash. Missing file means unknown (old packs);
  not “safe.”
- **Several components per pack.** One `pkg:*` is often several
  upstreams (`pkg:cc` is Zig + patchelf; `pkg:mesa` is Mesa + libdrm +
  32-bit bits). A list, not one string.
- **PURL is the machine id.** Each entry is `{name, version, purl}`
  plus optional `cpe`. `name` + `version` are what humans and agents
  print. Empty `purl` is refused when the file exists. These are a
  **claim**, not a proof the tree is that upstream (same class as T43
  `built_from`).
- **Advisories are not in the pack.** They must change after the bits
  freeze. Store `/oath/store/adv/<name>/<hash>.json`. Origin
  `{origin}/adv/<name>/<hash>.json` (T20 object storage). Hash of the
  pack does not cover this file.
- **No new kind. No new verb.** `oath get pkg:<name>` actual shows
  `components` (from the live tree) and `advisories` (from the adv
  file for that pin). `oath set` cannot write either. Apply of a pkg
  that has an origin URL best-effort wget’s the adv file; failure does
  not refuse apply (offline still works). Empty or missing adv file
  is unknown-or-none, never a safety proof.
- **Guest does not call OSV/NVD.** Independence. A host or origin
  publisher may query feeds using `components.json` and write hash
  advisories. The guest only fetches those files.
- **`oath apply` is how a live box learns a new advisory** for a pack
  it already has (same as everything else). Not `oath get` (get is
  read). Not a `svc`.

---

## Pack file (`components.json`)

UTF-8 JSON, no BOM, newline at EOF. Pretty-print, 2-space indent, no
trailing commas. Object key order **only:** `components`. Entries
sorted by `purl`. Entry key order **only:** `name`, `version`, `purl`,
optional `cpe`.

```
{
  "components": [
    {
      "name": "zig",
      "version": "0.16.0",
      "purl": "pkg:generic/zig@0.16.0"
    }
  ]
}
```

`purl` starts with `pkg:`. `cpe` is a CPE 2.3 string when we have one.

---

## Advisory file

`/oath/store/adv/<name>/<hash>.json` (and the same path under a T20
origin). Pretty-print, 2-space. Key order: `hash`, `pkg`, `advisories`.
Each advisory: `id`, `url`, optional `summary`.

```
{
  "hash": "sha256-<64 hex>",
  "pkg": "pkg:curl",
  "advisories": [
    {
      "id": "CVE-2024-6197",
      "url": "https://osv.dev/vulnerability/CVE-2024-6197",
      "summary": "…"
    }
  ]
}
```

`id` is a CVE / GHSA / OSV id (or an Oath id we minted). `url` is
https. The file is keyed by **pack hash**, not by CVE: one hash can
list several ids; one CVE can appear on several hashes as separate
files. `hash` / `pkg` inside the file must match the path.

---

## Catalog

**desired:** unchanged (no components, no advisories, no version).

**actual:** `components` copied from the live pack’s `components.json`
(empty if missing). `advisories` copied from
`/oath/store/adv/<name>/<live-hash>.json` when that file exists
(omitted if never fetched). Each realization summary may include
that tree’s `components`.

sola-oath reads those actual fields. A non-empty advisory list is a
danger chip. Open advisory `url` is the same `xdg-open` path as
`pkg.home`.

---

## Courage test (v0)

1. A pack tree with `components.json` hashes differently from the
   same tree without it. `oath get pkg:<name>` actual `components`
   matches the file. Missing file → empty list.
2. Writing `/oath/store/adv/<name>/<hash>.json` makes `oath get`
   actual `advisories` list that CVE. Apply with an origin URL wget’s
   `{origin}/adv/<name>/<hash>.json`; wget failure does not refuse.
3. `image/pack-cc.sh` (and rustc, bash) emit `components.json`.
4. sola-oath: pinned pack shows upstream `name version`; a fixture
   advisory paints a danger chip.

---

## Out

- Semver as install identity or `needs` ranges (T32 / T41)
- Slot-level `version` / `cpe` on desired
- Guest OSV/NVD client, CPE matching engine in PID 1
- New kind `adv` / `cve`
- New verb (`oath audit`, `oath vuln`)
- Treating empty components or empty advisories as “this pack is safe”
- Auto-present / auto-unpin when an advisory lands
- Signing the adv index (still T20 signatures)
