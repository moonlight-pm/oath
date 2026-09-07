# Spare Pitcairn as a render device

**Status:** idea (parked 2026-09-07). Not a freeze. Do not implement
from this file.
**Related:** T37 Steam nest; T38 7.3 GFX6 modifiers; canto dual
`1002:6810`.

---

Canto has two identical Pitcairn cards. Only one display will ever be
plugged in (today: Philips 1920×1080 on `card1` DP-10). `card0` has no
connectors in use. River already binds `WLR_DRM_DEVICES` to the first
connected card (`/lib/oath/run-compositor`).

**Not CrossFire / SLI.** amdgpu will not fuse the two chips into one
Vulkan device. Not worth the driver work.

**Maybe later: render on the dark GPU, present on the desk.** Gamescope
/ RADV on `card0` (`renderD128`), dmabuf import into River on `card1`.
Same GCN1, so GFX6 `IN_FORMATS` can match once RADV initializes. That
is PRIME, not a second monitor. Cheap only after Steam’s nest already
works on the display GPU.

Do not change `run-compositor` for this until the nest presents on one
card.
