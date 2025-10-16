# hl, your homelab CLI

Goal: A CLI to spin up, manage, and monitor apps on a homelab server.
Compiled into a single JS blob with esbuild and copied to the server, e.g. `/usr/local/bin/hl`.

Per-app config: small `homelab.yml` living on the server `home/felipecsl/prj/apps/<app>/homelab.yml`.

Server state: per-app directory `/srv/apps/<app>` holds `compose.yml`, `.env`, data volumes, logs.

Git hook: still server-side; it shells into hl deploy --hook with the refs and short SHA.
The CLI decides migrations/health/retagging/rollbacks.

Manages app lifecycle: deploy, start, stop, restart, status, logs, shell, exec, monitor.

# Command set (v1)

- `hl app create <app> [flags]` – sets up bare repo + hook + app dir + config.
- `hl init <app>` [--domain ... --port ...] – writes compose/env/systemd and brings it up.
- `hl deploy --hook` – invoked by post-receive; orchestrates build→push→migrate→health→retag→restart.
- `hl rollback <app> <sha>` – retag :latest to a previous image and restart, with health gate.
- `hl secrets set <app>` KEY=VALUE ... / ls / edit – manages the .env securely.
- `hl migrate <app>` [--image <tag>] – run DB migrations in a one-off container.
- `hl accessories add <app> postgres|redis [flags]` – drops a sidecar service (compose fragment) and wires env vars.

# Config file

```yaml
app: recipes
image: registry.lab.lima.gl/recipes
domain: recipes.lab.lima.gl
servicePort: 8080
resolver: myresolver
network: traefik_proxy
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
