// esbuild.config.mjs
import { build } from "esbuild";

await build({
  entryPoints: ["src/cli.ts"],
  outfile: "dist/hl.js",
  platform: "node",
  target: "esnext",
  format: "esm",
  bundle: true,
  minify: false,
  banner: { js: "#!/usr/bin/env node" },
});
console.log("Built dist/hl.js");
