// src/commands/deploy.ts
import { Command } from "commander";
import { loadConfig } from "../lib/config.js";
import {
  buildAndPush,
  retagLatest,
  restartCompose,
  runMigrations,
  tagFor,
} from "../lib/docker.js";
import { waitForHealthy } from "../lib/health.js";
import { log, ok } from "../lib/log.js";

export function register(prog: Command) {
  prog
    .command("deploy")
    .description(
      "Build->push->migrate->restart->health (invoke from post-receive)"
    )
    .requiredOption("--app <name>")
    .requiredOption("--sha <sha>")
    .option("--branch <name>", "git branch", "master")
    .option("--context <dir>", "build context", ".")
    .option("--dockerfile <path>", "Dockerfile path", "Dockerfile")
    .action(async (opts) => {
      const cfg = await loadConfig(opts.app);
      const tags = tagFor(cfg, opts.sha, opts.branch);

      log(`building ${cfg.app} ${opts.branch} (${opts.sha.slice(0, 7)})`);
      await buildAndPush({
        context: opts.context,
        dockerfile: opts.dockerfile,
        tags: [tags.sha, tags.branchSha, tags.latest],
        platforms: cfg.platforms,
      });

      log("running migrations");
      await runMigrations(cfg, tags.sha);

      log("retagging latest");
      await retagLatest(cfg.image, tags.sha);

      log("restarting compose");
      await restartCompose(cfg);

      log("waiting for health");
      await waitForHealthy(
        cfg.health.url,
        cfg.health.timeout,
        cfg.health.interval
      );

      ok("deploy complete");
    });
}
