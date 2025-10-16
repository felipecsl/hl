import { loadConfig } from "../lib/config.js";
import { retagLatest, restartCompose } from "../lib/docker.js";
import { waitForHealthy } from "../lib/health.js";
import { log, ok } from "../lib/log.js";
export function register(prog) {
    prog
        .command("rollback")
        .description("Retag :latest to a previous sha and restart (health-gated)")
        .argument("<app>")
        .argument("<sha>", "commit sha or image short tag")
        .action(async (app, sha) => {
        const cfg = await loadConfig(app);
        const from = `${cfg.image}:${sha.slice(0, 7)}`;
        log(`retagging ${from} -> ${cfg.image}:latest`);
        await retagLatest(cfg.image, from);
        log("restarting compose");
        await restartCompose(cfg);
        log("waiting for health");
        await waitForHealthy(cfg.health.url, cfg.health.timeout, cfg.health.interval);
        ok("rollback complete");
    });
}
//# sourceMappingURL=rollback.js.map