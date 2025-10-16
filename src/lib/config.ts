// src/lib/config.ts
import { promises as fs } from "fs";
import path from "path";
import yaml from "js-yaml";
import { z } from "zod";

export const HL_ROOT = "/home/felipecsl/prj/apps";

export const ConfigSchema = z.object({
  app: z.string(),
  image: z.string(),
  domain: z.string(),
  servicePort: z.number().int().positive(),
  resolver: z.string().default("myresolver"),
  network: z.string().default("traefik_proxy"),
  platforms: z.string().default("linux/amd64"),
  health: z.object({
    url: z.string().url(),
    interval: z.string().default("2s"),
    timeout: z.string().default("45s"),
  }),
  migrations: z.object({
    command: z.array(z.string()).default(["bin/rails", "db:migrate"]),
    env: z.record(z.string()).default({}),
  }),
  secrets: z.array(z.string()).default([]),
});
export type HLConfig = z.infer<typeof ConfigSchema>;

export async function loadConfig(app: string): Promise<HLConfig> {
  const p = path.join(HL_ROOT, app, "homelab.yml");
  const txt = await fs.readFile(p, "utf8");
  const raw = yaml.load(txt) as any;
  const cfg = ConfigSchema.parse(raw);
  return cfg;
}

export const appDir = (app: string) => path.join(HL_ROOT, app);
export const envFile = (app: string) => path.join(HL_ROOT, app, ".env");

// basic “2s/45s” → ms
export function parseDuration(s: string): number {
  const m = s.match(/^(\d+)(ms|s|m)$/);
  if (!m) throw new Error(`bad duration: ${s}`);
  const n = Number(m[1]);
  return m[2] === "ms" ? n : m[2] === "s" ? n * 1000 : n * 60_000;
}
