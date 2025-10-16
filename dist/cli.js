import { Command } from "commander";
import * as App from "./commands/app.js";
import * as Compose from "./commands/compose.js";
import * as Deploy from "./commands/deploy.js";
import * as Rollback from "./commands/rollback.js";
import * as Secrets from "./commands/secrets.js";
import * as Accessories from "./commands/accessories.js";
const prog = new Command().name("hl").description("Homelab deploy toolbox");
App.register(prog);
Compose.register(prog);
Deploy.register(prog);
Rollback.register(prog);
Secrets.register(prog);
Accessories.register(prog);
prog.parseAsync().catch((e) => {
    console.error(e?.stderr || e?.stack || e);
    process.exit(1);
});
//# sourceMappingURL=cli.js.map