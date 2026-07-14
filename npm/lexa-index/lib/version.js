import { readFileSync } from "node:fs";

const manifest = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);

if (typeof manifest.version !== "string" || manifest.version.length === 0) {
  throw new Error("Lexa package metadata does not contain a valid version.");
}

export const lexaVersion = manifest.version;
