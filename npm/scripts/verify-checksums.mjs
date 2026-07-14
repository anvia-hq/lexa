import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { parseArguments, requiredArgument } from "./arguments.mjs";

const parsed = parseArguments(process.argv.slice(2), new Set(["--directory"]));
const directory = resolve(requiredArgument(parsed, "directory"));
const checksumPath = join(directory, "SHA256SUMS");

for (const line of readFileSync(checksumPath, "utf8").split(/\r?\n/u)) {
  if (line.trim().length === 0) continue;
  const match = /^([a-fA-F0-9]{64})\s+\*?(.+)$/u.exec(line);
  if (match === null) throw new Error(`invalid checksum line: ${line}`);
  const actual = createHash("sha256")
    .update(readFileSync(join(directory, match[2])))
    .digest("hex");
  if (actual !== match[1].toLowerCase()) {
    throw new Error(`checksum mismatch for ${match[2]}`);
  }
}

console.log(`Verified checksums in ${checksumPath}.`);
