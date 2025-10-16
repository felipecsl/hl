import { getRefUpdatesFromStdin } from "../lib/git.js";
import { loadConfig } from "../lib/config.js";
import { buildAndPush, retagLatest, restartCompose } from "../lib/docker.js";
import { runMigrations } from "../lib/docker.js";
import { waitForHealthy } from "../lib/health.js";
import { log } from "../lib/log.js";
export function register(prog) {
    prog
        .command("deploy")
        .description("Run on post-receive: build, push, migrate, health-gated restart")
        .option("--hook", "read <old> <new> <ref> from stdin")
        .option("--app <name>", "app name (fallback when not in hook)")
        .action(async (opts) => {
        let app = opts.app;
        let sha = "";
        let branch = "";
        if (opts.hook) {
            const updates = await getRefUpdatesFromStdin();
            const head = updates.find((u) => u.ref.startsWith("refs/heads/"));
            if (!head)
                return;
            sha = head.new;
            branch = head.ref.replace("refs/heads/", "");
            app = app ?? inferAppFromGitDir();
        }
        else {
            if (!app)
                throw new Error("need --app");
            sha = process.env.SHA ?? "";
            branch = process.env.BRANCH ?? "master";
        }
        const cfg = await loadConfig(app);
        const tagSha = `${cfg.image}:${sha.slice(0, 7)}`;
        const tagBranchSha = `${cfg.image}:${branch}-${sha.slice(0, 7)}`;
        const tagLatest = `${cfg.image}:latest`;
        log(`Building ${cfg.app} at ${branch} (${sha.slice(0, 7)})`);
        await buildAndPush({
            context: ".",
            dockerfile: "Dockerfile",
            tags: [tagSha, tagBranchSha, tagLatest],
            platforms: cfg.platforms,
        });
        await runMigrations(cfg, tagSha);
        await retagLatest(cfg, tagSha);
        await restartCompose(cfg);
        await waitForHealthy(cfg.health.url, cfg.health.timeoutMs, cfg.health.intervalMs);
        log("Deploy complete ✅");
    });
}
//# sourceMappingURL=deploy.js.map