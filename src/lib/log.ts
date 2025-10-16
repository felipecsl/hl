// src/lib/log.ts
import { bold, gray, green, red, yellow } from "kleur/colors";
export const log = (m: string) => console.log(gray("•"), m);
export const ok = (m: string) => console.log(green("✓"), bold(m));
export const warn = (m: string) => console.log(yellow("!"), m);
export const err = (m: string) => console.error(red("x"), m);
