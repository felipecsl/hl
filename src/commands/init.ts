// src/commands/compose.ts
import { Command } from "commander";
import { promises as fs } from "fs";
import path from "path";
import { appDir } from "../lib/config.ts";
import { writeUnit } from "../lib/systemd.ts";
import { log, ok } from "../lib/log.ts";

export function register(prog: Command) {
  prog
    .command("init")
    .description("Initializes a new app with its configuration files")
    .requiredOption("--app <name>")
    .requiredOption("--image <ref>")
    .requiredOption("--domain <host>")
    .requiredOption("--port <num>", "internal container port")
    .option("--network <name>", "traefik network", "traefik_proxy")
    .option("--resolver <name>", "acme resolver", "myresolver")
    .action(async (opts) => {
      const d = appDir(opts.app);
      await fs.mkdir(d, { recursive: true });
      const envPath = path.join(d, ".env");
      try {
        await fs.access(envPath);
      } catch {
        await fs.writeFile(
          envPath,
          `APP=${opts.app}\nDOMAIN=${opts.domain}\nSERVICE_PORT=${opts.port}\n`
        );
      }

      const compose = `version: "3.9"
services:
  ${opts.app}:
    image: ${opts.image}:latest
    restart: unless-stopped
    env_file: [.env]
    networks: [${opts.network}]
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.${opts.app}.rule=Host(\\\`${"${DOMAIN}"}\\\`)"
      - "traefik.http.routers.${opts.app}.entrypoints=websecure"
      - "traefik.http.routers.${opts.app}.tls.certresolver=${opts.resolver}"
      - "traefik.http.services.${
        opts.app
      }.loadbalancer.server.port=${"${SERVICE_PORT}"}"
networks:
  ${opts.network}:
    external: true
    name: ${opts.network}
`;
      await fs.writeFile(path.join(d, "compose.yml"), compose);
      const unit = await writeUnit(opts.app);
      log(`wrote ${path.join(d, "compose.yml")} and ${envPath}`);
      ok(`enabled ${unit}`);
    });
}
