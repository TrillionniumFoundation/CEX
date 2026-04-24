# Local Repo Role

This machine's canonical local CEX workspace is:

- `/data/home-data/CEX`

The path below is only an alias to the same directory:

- `/home/qian-qi/CEX` -> symlink to `/data/home-data/CEX`

Practical rule:

- Treat `/data/home-data/CEX` as the primary path when starting services, checking logs, reading `.env`, or referring to runtime files.
- Do not treat `/home/qian-qi/CEX` as a second independent copy.

Verified on 2026-04-18:

- `/home/qian-qi/CEX` is a symlink to `/data/home-data/CEX`
- The earlier apparent "two copies" were actually one repo plus one alias path
