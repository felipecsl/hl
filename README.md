# `hl` — A tiny, deterministic “git-push deploys” CLI for single-host deployments

**Goal:** Keep deployments on a single host dead-simple, explicit, and reliable — without adopting a full orchestrator.

---

## Motivation & Goals

**What this solves**

- You have a single VPS/home server and multiple apps.
- You want **Heroku-style** “`git push` → build → deploy”, but:

  - no complex control planes,
  - no multi-host orchestration,
  - no hidden daemons updating containers behind your back.

**Design goals**

- **Deterministic:** deploy exactly the pushed commit, no working tree drift.
- **Explicit:** no Watchtower; restarts are performed by `hl`.
- **Boring primitives:** Git, Docker (Buildx), Traefik, Docker Compose, systemd.
- **Ergonomics:** one per-app folder on the server (`/home/<user>/hl/apps/<app>`), one systemd unit, minimal YAML.
- **Small blast radius:** per-app everything (compose/env/config) — easy to reason about and recover.

---

## How It Works

**Core flow**

1. **Push:** You push to a **bare repo** on the server (e.g., `/home/<user>/hl/git/<app>.git`).
2. **Hook → `hl deploy`:** The repo’s `post-receive` hook invokes `hl deploy` with `--sha` and `--branch`.
3. **Export commit:** `hl` **exports that exact commit** (via `git archive`) to an **ephemeral build context**.
4. **Build & push image:** Docker **Buildx** builds and pushes tags:

   - `:<shortsha>`, `:<branch>-<shortsha>`, and `:latest`.

5. **Migrations (optional):** `hl` runs DB migrations in a one-off container using the new image tag.
6. **Retag and restart:** `hl` **retags `:latest`** to the new sha and **restarts** the app using **systemd** (which runs `docker compose` under the hood).
7. **Health-gate:** `hl` waits until the app is healthy. Deploy completes only once healthy.

**Runtime layout (per app)**

```
/home/<user>/hl/apps/<app>/
  compose.yml              # app service + Traefik labels
  compose.<accessory>.yml  # e.g., compose.postgres.yml
  .env                     # runtime secrets (0600)
  hl.yml                   # server-owned app config
  pgdata/ ...              # volumes (if using Postgres)
systemd: app-<app>.service # enabled at boot
```

**Networking & routing**

- Traefik runs separately and exposes `web`/`websecure`.
- Apps join a shared Docker network (e.g., `traefik_proxy`) and advertise via labels.
- Certificates are issued by ACME (e.g., Route53 DNS challenge).

---

## How It’s Different From Existing Tools

| Tool                 | What it is                                                 | Where `hl` differs                                                                                                               |
| -------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| **Watchtower**       | Image watcher that auto-updates containers                 | `hl` **does not auto-update**. Deploys are explicit and health-gated.                                                            |
| **Kamal**            | SSH deploy orchestrator (blue/green, fan-out, hooks)       | `hl` intentionally avoids multi-host/fleet features and blue/green. It’s a **single-host release tool** with simpler ergonomics. |
| **Docker Swarm/K8s** | Schedulers with service discovery and reconciliation loops | `hl` doesn’t introduce a scheduler. It leans on systemd + compose for simple, predictable runtime.                               |

**Bottom line:** `hl` is a small, single-host release manager that turns a Git push into a reproducible build and a clean, health-checked restart — with Traefik for ingress. No magic daemons, no control plane.

---

## Pros / Cons of the Approach

**Pros**

- **Simplicity:** Git hooks + Docker Buildx + Compose + systemd.
- **Deterministic builds:** every deploy uses `git archive` of the exact commit.
- **Fast rollback:** `hl rollback <app> <sha>` retags and health-checks.
- **Clear logs:** `journalctl -u app-<app>.service` for runtime; deploy logs in hook/CLI output.
- **Separation of concerns:** build (ephemeral) vs. runtime (per-app directory).
- **Server-owned config:** domains, networks, health, secrets stay off the image.

**Cons / Trade-offs**

- **No blue/green:** restarts are in-place (health-gated, but not traffic-switched).
- **Single host:** no parallel fan-out or placement strategies.
- **Manual accessories:** DBs/Redis are compose fragments, not managed clusters.
- **Layer caching:** ephemeral build contexts reduce cache reuse (you can configure a persistent workspace if needed).

---

## Configuration (`hl.yml`)

> **Server-owned** file at `/home/<user>/hl/apps/<app>/hl.yml`.

```yaml
app: recipes
image: registry.example.com/recipes
domain: recipes.example.com
servicePort: 8080
resolver: myresolver # Traefik ACME resolver name
network: traefik_proxy # Docker network shared with Traefik
platforms: linux/amd64 # Buildx platforms

health:
  url: http://recipes:8080/healthz
  interval: 2s
  timeout: 45s

migrations:
  command: ["bin/rails", "db:migrate"]
  env:
    RAILS_ENV: "production"

secrets:
  - RAILS_MASTER_KEY
  - SECRET_KEY_BASE
```

---

## Health Checks

`hl` runs a short-lived `curl` container on the **app network** to hit `http://<service>:<port><path>`. Works even when nothing is published on host ports.

**Optional container healthcheck** in `compose.yml` keeps startup ordering crisp:

```yaml
services:
  recipes:
    healthcheck:
      test:
        [
          "CMD-SHELL",
          "wget -qO- http://localhost:8080/healthz >/dev/null 2>&1 || exit 1",
        ]
      interval: 5s
      timeout: 3s
      retries: 10
```

---

## Accessories (Example: Postgres)

`hl accessory add <app> postgres` will:

- Write `compose.postgres.yml` with a healthy `pg` service on the same network.
- Add `depends_on: { pg: { condition: service_healthy } }` to your app (via the fragment).
- Generate/update `.env` with:

  - `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB`
  - `DATABASE_URL=postgres://USER:PASSWORD@pg:5432/DB`

- Patch the systemd unit to run with **both** files:
  `docker compose -f compose.yml -f compose.postgres.yml up -d`
- Restart the unit.

> Same pattern can add **Redis** (`compose.redis.yml`, `REDIS_URL=redis://redis:6379/0`) and others.

---

## Using `hl` (Typical Workflow)

### 1) Bootstrap an app (one-time, on the server)

```bash
# Create runtime home, compose, hl.yml, systemd
hl init \
  --app recipes \
  --image registry.example.com/recipes \
  --domain recipes.example.com \
  --port 8080
```

This creates:

- `/home/<user>/hl/apps/recipes/{compose.yml,.env,hl.yml}`
- `app-recipes.service` (enabled)

### 2) Environment Variables

Add optional `--build` for build-time env vars (e.g., docker build secrets).

```bash
hl env set [--build] recipes RAILS_MASTER_KEY=... SECRET_KEY_BASE=...
hl env ls recipes  # prints keys with values redacted
```

### 3) Add Postgres (optional)

```bash
hl accessory add recipes postgres --version 16
# Writes compose.postgres.yml, updates systemd, restarts.
```

### 4) Create a bare repo + hook (one-time)

On the server:

```
/home/<user>/hl/git/recipes.git/hooks/post-receive
```

Triggers `hl deploy --app <appname> --sha "$newrev" --branch "$branch"`

### 5) Push to deploy

```bash
# on your laptop
git remote add production ssh://<user>@<host>/home/<user>/hl/git/recipes.git
git push production master
```

The pipeline:

- Exports the pushed commit
- Builds & pushes image (`:<sha>`, `:<branch>-<sha>`, `:latest`)
- Runs migrations on `:<sha>`
- Retags `:latest` → `:<sha>`
- Restarts `app-recipes.service`
- Waits for health

### 6) Rollback

```bash
hl rollback recipes eef6fc6
```

Retags `:latest` to the specified sha, restarts, and health-checks.

---

## Available Commands (Snapshot)

> **Command names/flags may differ in your Rust implementation, but this is the intended surface:**

- `hl init --app <name> --image <ref> --domain <host> --port <num> [--network traefik_proxy] [--resolver myresolver]`
  Create `compose.yml`, `.env`, `hl.yml`, and systemd unit.

- `hl deploy --app <name> --sha <sha> [--branch <name>]`
  Export commit → build & push → migrate → retag → restart (systemd) → health-gate.

- `hl rollback <app> <sha>`
  Retag `:latest` → `<sha>`, restart, health-gate.

- `hl secrets set <app> KEY=VALUE [KEY=VALUE ...]`
  Update the app’s `.env` (0600).
  `hl secrets ls <app>` to list keys redacted.

- `hl accessory add <app> postgres [--version <v>] [--user <u>] [--db <name>] [--password <p>]`
  Add Postgres as an accessory and wire `DATABASE_URL`.

- `hl accessory add <app> redis [--version <v>]`
  Add Redis as an accessory and wire `REDIS_URL`.

---

## Example `compose.web.yml` (app)

```yaml
services:
  recipes:
    image: registry.example.com/recipes:latest
    restart: unless-stopped
    env_file: [.env]
    networks: [traefik_proxy]
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.recipes.rule=Host(`recipes.example.com`)"
      - "traefik.http.routers.recipes.entrypoints=websecure"
      - "traefik.http.routers.recipes.tls.certresolver=myresolver"
      - "traefik.http.services.recipes.loadbalancer.server.port=${SERVICE_PORT}"

networks:
  traefik_proxy:
    external: true
    name: traefik_proxy
```

---

## Security & Operational Notes

- **Env vars:** keep in `.env` with mode `0600`. Do **not** bake secrets into images.
- **Registry auth:** the server must be logged in to your registry prior to deploys.
- **Traefik network:** ensure **one canonical network name** (e.g., `traefik_proxy`) shared by Traefik and apps.
- **Backups:** if using Postgres accessory, back up `pgdata/` and consider nightly `pg_dump`.
- **Layer cache:** if builds become slow, configure a persistent build workspace for better cache reuse.

---

## Roadmap / Next Steps

- **Accessories:** Redis helper (compose fragment + `REDIS_URL`), S3-compatible storage docs.
- **Hooks:** `preDeploy`/`postDeploy` (assets precompile, cache warmers).
- **Diagnostics:** `hl status/logs` wrapping `systemctl`/`journalctl` and `docker compose ps/logs`.
- **Rollback UX:** `hl releases <app>` to list recent SHAs/tags with timestamps.
- **Build cache toggle:** support persistent build workspace path in `homelab.yml`.
- **Backup tasks:** `hl pg backup/restore` helpers.
- **CI bridge:** optional GitHub Actions job that invokes the server over SSH and runs `hl deploy`.

---

## Architecture (At a Glance)

```
Laptop                                     Server
------                                     -------------------------------------
git push  ───────────────────────────────▶  bare repo: <app>.git
                                           post-receive → hl deploy --app --sha --branch
                                           ├─ export commit (git archive) → ephemeral dir
                                           ├─ docker buildx build --push (:<sha>, :<branch>-<sha>, :latest)
                                           ├─ run migrations (docker run ... :<sha>)
                                           ├─ retag :latest → :<sha> + push
                                           ├─ systemctl restart app-<app>.service
                                           └─ health-gate (docker-mode or http-mode)

Runtime
-------
Traefik ◀─────────────── docker network (traefik_proxy) ────────────────▶ App container
                                                      └──────(optional)▶ Postgres accessory
```

---

## FAQ

**Q: Where does the build context come from?**
A: From the **bare repo** you pushed to. `hl` uses `git archive` for the exact commit — no persistent working tree.

**Q: Why not Watchtower for restarts?**
A: To keep rollouts **explicit** and **health-gated** in one place (the deploy command).

**Q: Can I pin a version?**
A: Yes. Use `docker image` tags directly or `hl rollback <sha>` to retag `:latest` to a known-good image.

**Q: Can I use a public URL for health checks?**
A: Yes — set `health.mode: http` with a URL. Docker-mode is preferred for independence from DNS/ACME.

---

## License / Contributing

- MIT
- Contributions welcome — especially adapters for accessories (Redis), backup helpers, and CI bridges.
