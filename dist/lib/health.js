import http from "http";
import { parseDuration } from "./config.js";
export async function waitForHealthy(url, timeout, interval) {
    const timeoutMs = parseDuration(timeout);
    const intervalMs = parseDuration(interval);
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
        const ok = await ping(url);
        if (ok)
            return;
        await new Promise((r) => setTimeout(r, intervalMs));
    }
    throw new Error(`health check timed out: ${url}`);
}
function ping(urlStr) {
    return new Promise((resolve) => {
        const req = http.get(urlStr, (res) => {
            res.resume();
            resolve(res.statusCode >= 200 && res.statusCode < 400);
        });
        req.on("error", () => resolve(false));
        req.setTimeout(3000, () => {
            req.destroy();
            resolve(false);
        });
    });
}
//# sourceMappingURL=health.js.map