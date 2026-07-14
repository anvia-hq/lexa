import { readFileSync } from "node:fs";

export { lexaVersion } from "./lib/version.js";

export const lexaSkill = readFileSync(
  new URL("./skill/SKILL.md", import.meta.url),
  "utf8",
);
