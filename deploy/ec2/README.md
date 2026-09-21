# EdgeFabric EC2 deploy

Deploy EdgeFabric to the **same R&D EC2** used by eGenie / WiseEars, with the same SSH key and build routing.

## Quick start

```bash
cd /path/to/EdgeFabric

# One-time: connection settings (same host as eGenie)
cp deploy/ec2/host.env.example deploy/ec2/host.env
# edit host.env if your key or host differ

chmod +x deploy/ec2/deploy-edgefabric.sh
./deploy/ec2/deploy-edgefabric.sh
```

## Connection (shared with eGenie)

| Setting | Default |
| --- | --- |
| Host | `100.52.147.238` |
| SSH key | `/Users/vijay/rnd/projects/VeerSetuHost.pem` |
| User | `ubuntu` |
| Public URL | `https://edgefabric.veerlabs.solutions` |

Override via `deploy/ec2/host.env` or env vars:

```bash
DEPLOY_HOST=100.52.147.238 DEPLOY_KEY=/Users/vijay/rnd/projects/VeerSetuHost.pem ./deploy/ec2/deploy-edgefabric.sh
```

`host.env` is gitignored — copy from `host.env.example`.

## What gets installed

| Component | Location | Port |
| --- | --- | --- |
| **ef-api** | `/usr/local/bin/ef-api` | `127.0.0.1:6060` |
| Source tree | `/opt/edgefabric` | — |
| Config | `/etc/edgefabric/edgefabric.env` | — |
| Postgres / Redis / NATS | Docker (`deploy/ec2/infra-compose.ec2.yml`) | `127.0.0.1` only |
| systemd | `ef-api.service` | — |
| Caddy (if present) | Appends `edgefabric.veerlabs.solutions` → `:6060` | 443 |

Optional agent on the same host:

```bash
DEPLOY_AGENT=1 ./deploy/ec2/deploy-edgefabric.sh
```

## Linux builds (same as eGenie)

| `BUILD_MODE` | Behavior |
| --- | --- |
| `auto` (default) | Build on EC2 when SSH works; else LAN `192.168.29.78` |
| `ec2` | Force EC2 compile |
| `lan` | Build on `.78`, install when EC2 is back |

```bash
# EC2 stopped — build on LAN, then start EC2 and re-run:
BUILD_MODE=lan ./deploy/ec2/deploy-edgefabric.sh

# Skip compile (config/systemd only):
SKIP_BUILD=1 ./deploy/ec2/deploy-edgefabric.sh

# Force rebuild:
FORCE_BUILD=1 ./deploy/ec2/deploy-edgefabric.sh
```

## Prerequisites on EC2

The eGenie host bootstrap already provides **Docker**. If this is a fresh box:

```bash
# From egenie repo
scp -i /Users/vijay/rnd/projects/VeerSetuHost.pem -r deploy/ec2 ubuntu@HOST:/tmp/egenie-ec2
ssh -i /Users/vijay/rnd/projects/VeerSetuHost.pem ubuntu@HOST 'sudo bash /tmp/egenie-ec2/bootstrap-host.sh'
```

EdgeFabric does **not** conflict with WiseEars (`8080`), eGenie API (`8081`), or Srotiva (`8090`).

## DNS

Add an A record (same elastic IP as eGenie):

```
edgefabric.veerlabs.solutions  →  100.52.147.238
```

Caddy on the host terminates TLS. The deploy script **appends** the EdgeFabric block; it does not replace the eGenie Caddyfile.

## Verify

On the host:

```bash
curl http://127.0.0.1:6060/health
sudo journalctl -u ef-api -f
```

From your laptop (after DNS):

```bash
curl https://edgefabric.veerlabs.solutions/health
```

## Idle shutdown

If eGenie idle-shutdown is enabled, deploy adds port **6060** to `MONITOR_PORTS` in `/etc/egenie/idle-shutdown.conf`.

## Files in this directory

| File | Purpose |
| --- | --- |
| `host.env.example` | SSH host, key, public URL template |
| `linux-build.sh` | EC2 vs LAN build routing (from eGenie) |
| `deploy-edgefabric.sh` | Main deploy script |
| `infra-compose.ec2.yml` | Postgres, Redis, NATS on localhost |

## Related

- eGenie EC2 docs: `../egenie/deploy/ec2/README.md` (sibling repo)
- Local dev: `./scripts/deploy.sh all`
