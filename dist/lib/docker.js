import { execa } from "execa";
import path from "path";
import { appDir, envFile } from "./config.js";
export async function buildAndPush(opts) {
    const args = ["buildx", "build", "--push"];
    if (opts.platforms)
        args.push("--platform", opts.platforms);
    for (const t of opts.tags)
        args.push("-t", t);
    if (opts.dockerfile)
        args.push("--file", opts.dockerfile);
    args.push(opts.context);
    await execa("docker", args, { stdio: "inherit" });
}
export async function retagLatest(image, fromTag) {
    await execa("docker", ["pull", fromTag], { stdio: "inherit" });
    await execa("docker", ["tag", fromTag, `${image}:latest`], {
        stdio: "inherit",
    });
    await execa("docker", ["push", `${image}:latest`], { stdio: "inherit" });
}
export async function restartCompose(cfg) {
    const dir = appDir(cfg.app);
    await execa("docker", ["compose", "-f", "compose.yml", "pull"], {
        cwd: dir,
        stdio: "inherit",
    });
    await execa("docker", ["compose", "-f", "compose.yml", "up", "-d"], {
        cwd: dir,
        stdio: "inherit",
    });
}
export async function runMigrations(cfg, imageTag) {
    const dir = appDir(cfg.app);
    const envPath = envFile(cfg.app);
    const envArgs = Object.entries(cfg.migrations.env).flatMap(([k, v]) => [
        "-e",
        `${k}=${v}`,
    ]);
    const runArgs = [
        "run",
        "--rm",
        "--env-file",
        envPath,
        "--network",
        cfg.network,
        imageTag,
        ...cfg.migrations.command,
    ];
    await execa("docker", ["run", ...envArgs, ...runArgs], {
        cwd: dir,
        stdio: "inherit",
    });
}
export async function inspectContainerNetworks(name) {
    const { stdout } = await execa("docker", [
        "inspect",
        name,
        "--format",
        "{{json .NetworkSettings.Networks}}",
    ]);
    return stdout;
}
export function tagFor(cfg, sha, branch) {
    const short = sha.slice(0, 7);
    return {
        sha: `${cfg.image}:${short}`,
        branchSha: `${cfg.image}:${branch}-${short}`,
        latest: `${cfg.image}:latest`,
    };
}
export function composePath(cfg) {
    return path.join(appDir(cfg.app), "compose.yml");
}
//# sourceMappingURL=docker.js.map