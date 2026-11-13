# Consort: Oath machines that work together

A **consort** is a set of Oath machines that know each other. Each member is
a **peer**. Peers find each other on the network, publish facts about
themselves, and accept the requests they choose to accept. Nothing in a
consort leads: every peer decides for itself.

**Status:** partly built. Decided by Joshua 2026-11-12 (R87 in
[../decisions.md](../decisions.md)). Built 2026-11-13: step 2 of the order
below, membership and read-only facts (`crates/oath-bus`,
`oath_core::consort`, `oath consort`, the `peer` kind); how to use it is in
[../manual/consort.md](../manual/consort.md). Updates on the bus, calls,
invitations, guests, cells on a peer and failover will be.

## What it is for, in order

1. **See and update.** One place (novus today, any peer in principle) shows
   every machine: what release it runs, what is waiting, what failed. Updates
   happen on their own and ask only when they would disrupt someone.
2. **Place work.** A cell runs on another peer that allows it. Not designed
   here yet.
3. **Survive a machine.** A service such as Wicket stays up while its machine
   reboots. Not designed here yet. Its way in, a Cloudflare tunnel, can run a
   `cloudflared` replica on each of two peers; its stored data (Postgres,
   Stalwart, the registry) is the hard part.

Each needs the one before. Membership, consent and the bus serve all three.

## Words

| Word | Is |
|---|---|
| consort | the peers on one signed membership log |
| peer | one member; `peer:<name>` in each machine's catalog (a tenth kind, R87) |
| `oath-bus` | the daemon on each peer, `svc:oath-bus`: its local hub and its links to the other peers |
| `oath call` | the verb that will send a request to a peer |

## Facts and observers (built)

`oath-bus` follows the shape of Sola's `sola-bus` and `sola-call`, in one
process with two planes, since the links between machines are the costly
part. The facts plane is built.

- **Records.** A record is a fact with a topic, a sequence number and its
  owner's signature. Each peer publishes records about itself (`status`,
  `services`, `packages`, `load`, `processes`, `update`; the manual lists
  their fields) and sends them to every member. A record is written only by its
  owner, the peer it describes, and a newer sequence number from the owner
  replaces the older, so facts need no agreement between peers and no leader. A
  peer that links late gets the current ones.
- **Observers.** A local client watches every record and every change of who is
  in the consort or online (`watch` on `/oath/run/bus.sock`). A desk's view of
  the consort is an observer on its own machine's hub.
- **Local programs** (`oath`, a desk app, later Wicket) reach their own
  machine's `oath-bus` over that socket, as Sola apps reach `sola-bus`.
- **Still one admin surface.** `oath consort`, `oath ls peer` and `oath get
  peer:<name>` are `oath` verbs, and `oath mcp` has a `consort` tool.
- Facts are read with `oath-core` from the machine's own catalog, as `oath get`
  reads them, and from `/proc`; `update` runs `oath update`'s preview.

## Calls (will be)

- A peer advertises the methods it accepts, each an `oath` verb: `get`,
  `logs`, `update`, and later `cell.run`. A request goes to one peer, and that
  peer checks it against its own policy. A peer that does not accept a method
  does not advertise it. `oath -p dulcia logs svc:wicket`, `oath call dulcia
  update`.
- Only failover (purpose 3) needs peers to agree, on which one runs a primary.
  Three peers (aulos, buccina, dulcia) can agree safely; two cannot without a
  third as tiebreaker.

## Admission: a signed membership log (built)

No consort key and no authority: membership is a log every member keeps
and checks, as Tailscale's Tailnet Lock and Keybase's device chains do.

- **Each peer has two keys of its own,** made on `oath-bus`'s first start in
  `/oath/bus/`, which never leave the machine: an Ed25519 key it signs with,
  written in SSH's format as publisher keys are (R72), and an X25519 key its
  links agree on. ed25519-dalek advises against one key for both, so the log
  binds the second to the first.
- **Every machine starts as a consort of one.** Its first log entry names
  itself, with a random salt; the consort's id is that entry's hash.
- **The log** is a graph of entries, each an admission or a removal signed by
  the member that made it, after the entries it had. Every peer replays it in
  one order (depth from the first entry, then hash), and an entry counts only
  when its signer is a member at that point, so any member can be lost and none
  is the authority. Two members never share a name.
- **Joining.** A lone machine asks a member it heard (`oath consort join`),
  signing an ask that names the consort and its link key. Any member admits it
  once the owner types the six-digit code the joining machine printed (`oath
  consort admit <name> --code …`), made from the consort's id and both its
  keys. The member holding the ask publishes it to the others, so the owner
  admits from whichever member he is at. The newcomer takes the consort's log.
- **Removal.** Any member may remove another, or leave. A removed key's
  signatures stop counting from that entry, so it cannot let itself back in; a
  later admission brings it back. A removed peer becomes a consort of one.
- Being admitted grants nothing by itself. Calls will add each peer's own policy
  on top.

**Will be:** invitations. A member mints a one-time invitation that expires.
Put in an install overlay, it lets the machine join on its first boot with no
further approval.

**Hardening, later** (Joshua, 2026-11-12: "right now this is just me and maybe
my brother up the street"). Today a compromised member can admit a rogue or
remove the others; it could already sign packages every machine takes
(`oath@novus` with `names="*"`), so the log is no weaker than that. When more
people join:

- only members the owner marks may admit;
- an admission or a removal takes two members' signatures;
- a removal beats a concurrent admission whatever the replay order (today the
  order settles a race);
- the desk's view alerts on every admission and removal;
- an invitation names the machine it is for (its host key), not just a secret;
- entries expire unless renewed, so a lost machine falls out on its own;
- `oath-bus` drops root (Joshua, 2026-11-13: fine for now): its network side as
  `void` in `/var/empty`, a root parent holding the keys and the log, as sshd
  and dhcpcd do. It runs as root today, since it reads the catalog and writes
  `peer:*` objects, and the updates step will run `oath update`.

## Links and discovery (built)

- **Link.** Peers connect directly to each other, each to every other, which is
  enough for well under fifty machines; of two peers, the one with the lower key
  dials. The link is a Noise XX handshake (as WireGuard uses) on TCP 6284, over
  each peer's X25519 link key: snow's X25519, ChaCha20-Poly1305 and BLAKE2s,
  with the kernel's random numbers. A link is whoever's link key the log gives
  the key the handshake proved, checked again whenever the log changes. Pings
  every 20 s; a link quiet for 60 s is dropped. A link is never a shell; SSH
  stays the way a person or agent logs in.
- **Discovery.** Each peer broadcasts a signed UDP beacon on 6284 every 5 s:
  its name, key, consort, member count and link port. A member's beacon gives
  its address; any other machine shows as nearby. A beacon whose clock is two
  minutes off is ignored. Our own beacon, not mDNS through Avahi (R57).
- **Off the LAN** (will be): beacons do not cross routers. A WireGuard tunnel
  carries them, or a peer is named by address and the others learn of it from
  that peer.

## Who reads what

- **Built:** every record names its audience, `consort` for every topic today,
  and goes only over links to members; every link is encrypted. A machine that
  is not a member gets nothing but the log that admits it.
- **Will be:** wider audiences, chosen by a fact's owner topic by topic. The
  sender decides who reads; the receiver decides which calls it accepts.
- **Will be:** sealed records, a payload encrypted to an audience's key, age's
  way, for when a record passes through a peer that should not read it (a
  relay, or a record held for a sleeping machine).
- A peer publishes process names, not command lines, which can hold secrets.

## Guests: another person's consort (will be)

A colleague who will not give SSH access keeps his own consort. A member of each
consort signs a guest entry naming the other, scoped to the topics it may read
and the calls it may make, each with its owner's approval. Only the peers on each side
that opt in carry the link, and each still applies its own policy. His side opens
one port or one tunnel to your peer, and nothing else.

## Updates on the bus (will be)

- **A doorbell, not a mailbox.** buccina's audit publishes to the origin as
  today, then announces the new release or heads on the bus. The announcement
  carries no authority: each peer still fetches from the origin and checks the
  signature against `host:local.publishers`. R72 stands, and a forged
  announcement can at most make a peer check.
- **Automatic and non-disruptive.** A peer takes what it may take on its own,
  when convenient, from a few settings of its own: on or off, a time window,
  what it holds back.
- **Disruptive** applies to desks, where someone may be at the machine: a
  reboot, and anything that takes down the desk or the seat. A desk publishes
  such an update as pending, and its owner approves it now or at a time, by an
  `oath call` to that peer. Any other peer does not reboot while a build or
  other declared work runs, and otherwise goes ahead.
- **Rings.** A peer may wait until the peers in the ring before it publish that
  they are healthy on the new release: canto and buccina first, then the
  servers, desks last. No leader orders it: each peer reads the others' records.
- A peer asleep with Wake-on-LAN (R84) may be woken in its window, update, and
  sleep again.

## Catalog

- **Built:** `peer:<name>`, an inventory kind as `dev` is: `oath-bus` makes one
  for each member, this machine included (`me`), and removes it when the member
  leaves. Desired is `present` alone, and false is refused. Actual: the key and
  its fingerprint, the address, `online`, `seen`, who admitted it and when, and
  its newest `status` record. No seed or overlay names one.
- **Will be:** desired fields for what this machine allows that peer, with
  calls; update settings on `host:local`, with updates on the bus.

## What it changes

- [agent.md](agent.md)'s "no new daemon, port or credential": the bus adds a
  daemon per peer, a port for links and beacons, and two keys per peer. SSH
  stays the transport for logins and `oath mcp`.
- A tenth kind, `peer`.
- R72 stands: packages still meet only at the origin.

## Order of work

1. aulos on Oath (done 2026-11-12, [../../hosts/aulos.md](../../hosts/aulos.md)).
2. Facts, read-only: peer keys and the membership log, the beacon, the links,
   each peer's records, `oath ls peer`; a desk app watches (built 2026-11-13;
   Sola's Oath app is its first observer).
3. Updates on the bus: the doorbell, each peer's settings, pending and approval,
   rings.
4. Calls to a peer beyond `update`.
5. Cells on a peer (aulos and dulcia), then Wicket's failover.

## Open

- Who places work: Oath for any payload, or Wicket over Oath's membership and
  consent ([scope.md](scope.md) keeps scheduling out of core).
