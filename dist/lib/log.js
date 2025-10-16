import { bold, gray, green, red, yellow } from "kleur/colors";
export const log = (m) => console.log(gray("•"), m);
export const ok = (m) => console.log(green("✓"), bold(m));
export const warn = (m) => console.log(yellow("!"), m);
export const err = (m) => console.error(red("x"), m);
//# sourceMappingURL=log.js.map