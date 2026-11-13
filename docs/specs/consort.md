# Consort: Oath machines that work together

A **consort** is a set of Oath machines that know each other. Each member is
a **peer**. Peers find each other on the network, publish facts about
themselves, and accept the requests they choose to accept. Nothing in a
consort leads: every peer decides for itself.

**Status:** will be; nothing is built. Decided by Joshua 2026-11-12 (R87 in
[../decisions.md](../decisions.md)). The first members are aulos and dulcia
(two OptiPlexes on `base`), then the other machines.

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
| consort | the set of peers admitted by one consort key |
| peer | one member; `peer:<name>` in each machine's catalog (a tenth kind, R87) |
| `oath-bus` | the daemon on each peer, `svc:oath-bus`: its local hub and its links to the other peers |
| `oath call` | the verb that sends a request to a peer |

## The bus (will be)

`oath-bus` follows the shape of Sola's `sola-bus` and `sola-call`, in one
process with two planes, since the links between machines are the costly
part.

- **Facts.** A record is a typed fact with a topic, keys and an owner. Records
  are sticky: a peer that joins late gets every current record. Each peer
  publishes records about itself: its profile and output, generation, drift,
  pending update, failed services, uptime, load, seat in use, a build running,
  and so on. A record is written only by its owner, the peer it describes. Copies
  elsewhere are replaced by the owner's newer sequence number, so facts need no
  agreement between peers and no leader.
- **Calls.** A peer advertises the methods it accepts, each an `oath` verb:
  `get`, `logs`, `update`, and later `cell.run`. A request goes to one peer, and
  that peer checks it against its own policy. A peer that does not accept a
  method does not advertise it.
- **Observers.** A local client can watch every record and a trace of calls.
  The desk's view of the consort is an observer on its own machine's hub.
- **Local programs** (`oath`, a desk app, Wicket) reach their own machine's
  `oath-bus` over a Unix socket, as Sola apps reach `sola-bus`.
- **Still one admin surface.** The bus is how `oath` reaches other machines,
  not a second interface: `oath ls peer`, `oath -p dulcia logs svc:wicket`,
  `oath call dulcia update`. An agent uses the same verbs.
- Only failover (purpose 3) needs peers to agree, on which one runs a primary.
  Three peers (aulos, buccina, dulcia) can agree safely; two cannot without a
  third as tiebreaker.

## Admission: the consort key (will be)

- One consort to start. Its key is held by the owner, and signs a certificate
  for each member: its name, its bus key, and how long the certificate holds.
  Keys and certificates use SSH's formats and are signed with `ssh-keygen`, as
  publisher keys are (R72). No sshd is involved.
- A machine joins by trusting the consort key and receiving its certificate:
  one step on the owner's machine and one on the new peer, however many peers
  there are. A revocation list, signed with the same key, removes a member.
- The key is needed to admit, not to run. A consort keeps working with the key
  offline.
- Being admitted grants nothing by itself. Each peer applies its own policy on
  top: which methods it advertises and to whom.

## Links and discovery (will be)

- **Link.** Peers connect directly to each other, each to every other, which is
  enough for well under fifty machines. The link is a Noise handshake (as
  WireGuard uses) on `oath-bus`'s own port, authenticated by the bus keys the
  consort key certified. `oath-bus` runs unprivileged, and a link can never
  become a shell. SSH stays the way a person or agent logs in.
- **Discovery.** Each peer sends a small signed UDP beacon on its LAN every few
  seconds: its name, key fingerprint and address. Peers that hear a member
  connect to it. A machine that is not admitted shows as nearby and not joined.
  Our own beacon, not mDNS through Avahi (R57).
- **Off the LAN** (later): beacons do not cross routers. A WireGuard tunnel
  carries them, or a peer is named by address and the others learn of it from
  that peer.

## Who reads what (will be)

- **Audiences.** Every record names who may read it. A hub forwards a record
  only to peers in its audience, and every link is encrypted. The default
  audience is the consort, and the owner of a fact chooses any wider audience
  topic by topic. The sender decides who reads; the receiver decides which calls
  it accepts.
- **Sealed records** (later): a payload encrypted to an audience's key, age's
  way, for when a record passes through a peer that should not read it (a relay,
  or a record held for a sleeping machine). The record format leaves room for
  it.
- By default a peer publishes process names, not command lines, which can hold
  secrets.

## Guests: another person's consort (later)

A colleague who will not give SSH access keeps his own consort. His consort key
signs a guest certificate for yours, scoped to the topics you may read and the
calls you may make, and yours does the same for him. Only the peers on each side
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

## Catalog (sketch, will be)

- `peer:<name>`: one per member this machine knows. Actual: its key
  fingerprint, address, certificate expiry, last seen, its newest records.
  Desired: what this machine allows that peer, defaulting to the consort policy.
- On `host:local`: the consort key this machine trusts (none: in no consort),
  and its update settings. Field names wait for the build.

## What it changes

- [agent.md](agent.md)'s "no new daemon, port or credential": the bus adds a
  daemon per peer, a port for links and beacons, and the consort key. SSH stays
  the transport for logins and `oath mcp`.
- A tenth kind, `peer`.
- R72 stands: packages still meet only at the origin.

## Order of work

1. aulos on Oath (done 2026-11-12, [../../hosts/aulos.md](../../hosts/aulos.md)).
2. Facts, read-only: the consort key and certificates, the beacon, the links,
   each peer's records, `oath ls peer`; a desk app watches.
3. Updates on the bus: the doorbell, each peer's settings, pending and approval,
   rings.
4. Calls to a peer beyond `update`.
5. Cells on a peer (aulos and dulcia), then Wicket's failover.

## Open

- Who places work: Oath for any payload, or Wicket over Oath's membership and
  consent ([scope.md](scope.md) keeps scheduling out of core).
- The Noise library and the port: through intake (R57).
- The record schema and the first topics.
