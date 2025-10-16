import { promises as fs } from "fs";
import { appDir, envFile } from "../lib/config.js";
export function register(prog) {
    const cmd = prog.command("secrets").description("Manage .env secrets");
    cmd
        .command("set")
        .argument("<app>")
        .argument("<pairs...>", "KEY=VALUE")
        .action(async (app, pairs) => {
        const f = envFile(app);
        await fs.mkdir(appDir(app), { recursive: true });
        try {
            await fs.access(f);
        }
        catch {
            await fs.writeFile(f, "");
        }
        const text = await fs.readFile(f, "utf8");
        const lines = text.split("\n");
        const map = new Map();
        for (const l of lines)
            if (l && !l.startsWith("#")) {
                const i = l.indexOf("=");
                if (i > 0)
                    map.set(l.slice(0, i), l.slice(i + 1));
            }
        for (const kv of pairs) {
            const i = kv.indexOf("=");
            if (i < 1)
                throw new Error(`bad pair: ${kv}`);
            map.set(kv.slice(0, i), kv.slice(i + 1));
        }
        const out = Array.from(map.entries())
            .map(([k, v]) => `${k}=${v}`)
            .join("\n") + "\n";
        await fs.writeFile(f, out, { mode: 0o600 });
        console.log(`updated ${f}`);
    });
    cmd
        .command("ls")
        .argument("<app>")
        .action(async (app) => {
        const txt = await fs.readFile(envFile(app), "utf8").catch(() => "");
        for (const l of txt.split("\n")) {
            if (!l || l.startsWith("#"))
                continue;
            const i = l.indexOf("=");
            if (i > 0)
                console.log(l.slice(0, i) + "=***");
        }
    });
}
//# sourceMappingURL=secrets.js.map