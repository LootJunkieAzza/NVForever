# Security

## Reporting a problem

If you find a security problem in NVForever, please report it privately: use
**[Report a vulnerability](https://github.com/LootJunkieAzza/NVForever/security/advisories/new)**
on the repository's Security tab. Don't open a public issue for it.

Examples of what to report:

- a crafted plugin, archive, mesh, save or other file that makes NVForever run code, or
  read or write files outside its own folders;
- NVForever writing into the game folder or `Documents\My Games\FalloutNV`, which it
  must never do ([docs/design/install-layout.md](docs/design/install-layout.md));
- a release download that doesn't match what the release workflow built.

Ordinary crashes on files that aren't crafted to attack it are bugs. Report those with
the [issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose).

## What to expect

NVForever is a volunteer project. Reports are read as time allows, usually within a
couple of weeks. A fix goes into the next release, and the advisory is published once
that release is out. Only the latest release is supported.
