# peer

A member of this machine's consort (R87): Oath machines that know each
other, found on the LAN and linked by `oath-bus` (`svc:oath-bus`). The
catalog is the inventory; `oath consort` changes who is in it.

**When to use:** see the other machines and what they run (`oath ls peer`,
`oath get peer:aulos`), and `oath consort` for the whole picture: members,
machines nearby that are not, and machines asking to join.

**When not:** do not `ssh` round the machines to compare them; their
facts are here. `oath set peer:x present=false` is refused: `sudo oath
consort remove x` removes a member, for every member at once.

`oath-bus` makes the objects from its membership log, a chain of signed
entries every member keeps; no machine leads, and any member admits a
machine at the owner's word. This machine is a peer of its own (`me`).

## Fields

- `present` — in the consort. `false` is refused.

Actual also has `me`, the signing `key` (an `.pub` line) and its
`fingerprint`, the `address` its link comes from, `online`, `seen` (when
its link last went down), `admitted_by` and `admitted`, and `status`: its
newest status record (profile, output, generation, drift, failed
services, boot time, kernel).

## Joining

```
sudo oath consort join <peer>         # on the new machine: prints a code
oath consort                          # on any member: the ask, and its code
sudo oath consort admit <name> --code <code>
```

## Example

```
oath ls peer
oath get peer:aulos
oath consort --json
```
