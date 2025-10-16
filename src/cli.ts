// src/cli.ts
import { Command } from "commander";
import { register as regDeploy } from "./commands/deploy.js";
import { register as regCompose } from "./commands/compose.js";
import { register as regRollback } from "./commands/rollback.js";
import { register as regSecrets } from "./commands/secrets.js";

const prog = new Command().name("hl").description("Homelab deploy toolbox");
regDeploy(prog);
regCompose(prog);
regRollback(prog);
regSecrets(prog);
prog.parseAsync().catch((e) => {
  console.error(e?.stderr || e?.stack || e);
  process.exit(1);
});
