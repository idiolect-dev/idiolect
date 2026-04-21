# Deploying idiolect

Two daemons are shipped as deployable artifacts:

- **`idiolect-orchestrator`** — read-only HTTP catalog over the firehose.
- **`idiolect-observer`** — firehose-to-observation publisher.

Both are Rust binaries, both run long-lived, both speak to the
atproto firehose via `tapped`. Neither is a service of record — a
deployment losing its data directory replays from the firehose.

## Environment variables

### Orchestrator

| Variable | Default | Meaning |
|---|---|---|
| `IDIOLECT_HTTP_ADDR` | `127.0.0.1:8787` | HTTP bind address. |
| `IDIOLECT_CATALOG_SQLITE` | (unset) | Path to sqlite file holding the persisted catalog mirror. When unset the catalog runs in-memory and is lost on restart. |
| `IDIOLECT_CURSORS` | (unset) | Path to the firehose cursor store. When unset, the daemon starts from the retention floor on every restart. |
| `RUST_LOG` | `info` | Tracing filter. |

### Observer

| Variable | Default | Meaning |
|---|---|---|
| `IDIOLECT_OBSERVER_DID` | *required* | DID the observer publishes under. |
| `IDIOLECT_PDS_URL` | (unset) | PDS base URL. When unset the observer runs with `InMemoryPublisher` and no records persist. |
| `IDIOLECT_OBSERVER_CURSORS` | (unset) | Path to the cursor store. |
| `IDIOLECT_FLUSH_EVENTS` | `100` | Flush interval by event count. Set `0` to disable. |
| `RUST_LOG` | `info` | Tracing filter. |

## Docker

Build the images from the repo root:

```sh
docker build -f deploy/docker/orchestrator.Dockerfile -t idiolect-orchestrator:local .
docker build -f deploy/docker/observer.Dockerfile -t idiolect-observer:local .
```

Both images are distroless (no shell, no package manager). State is
expected to live on a named volume mounted at `/var/lib/idiolect/*`.

Run:

```sh
docker run --rm -p 8787:8787 \
  -v idiolect-orch-data:/var/lib/idiolect/orchestrator \
  -e IDIOLECT_CATALOG_SQLITE=/var/lib/idiolect/orchestrator/catalog.db \
  -e IDIOLECT_CURSORS=/var/lib/idiolect/orchestrator/cursors.db \
  -e IDIOLECT_HTTP_ADDR=0.0.0.0:8787 \
  idiolect-orchestrator:local

docker run --rm \
  -v idiolect-obs-data:/var/lib/idiolect/observer \
  -e IDIOLECT_OBSERVER_CURSORS=/var/lib/idiolect/observer/cursors.db \
  -e IDIOLECT_PDS_URL=https://bsky.social \
  -e IDIOLECT_OBSERVER_DID=did:plc:your-observer-did \
  idiolect-observer:local
```

## systemd

The unit files in `deploy/systemd/` assume:

- A system user `idiolect` with a home in `/var/lib/idiolect`.
- Binaries installed at `/usr/local/bin/idiolect-orchestrator` and
  `/usr/local/bin/idiolect-observer`.

Install:

```sh
sudo useradd --system --home-dir /var/lib/idiolect --shell /usr/sbin/nologin idiolect
sudo install -m 0644 deploy/systemd/idiolect-orchestrator.service /etc/systemd/system/
sudo install -m 0644 deploy/systemd/idiolect-observer.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now idiolect-orchestrator
```

Override environment variables via `systemctl edit`. Each unit
declares `StateDirectory=idiolect/<daemon>` so systemd creates and
chowns the state directory automatically.

## Upgrades

Both daemons are stateless over their sqlite stores — an upgrade
replaces the binary, restarts the service, and resumes from the
persisted cursor. No migrations run on startup; schema changes to
the persistent stores ship as explicit migrations in the
`idiolect-migrate` crate when they land.
