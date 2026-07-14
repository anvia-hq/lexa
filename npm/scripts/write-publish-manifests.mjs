import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { parseArguments, requiredArgument } from "./arguments.mjs";
import {
  packageDefinitions,
  stagedPackageDirectory,
} from "./package-definitions.mjs";

const parsed = parseArguments(
  process.argv.slice(2),
  new Set(["--stage", "--tarballs"]),
);
const stage = resolve(requiredArgument(parsed, "stage"));
const tarballs = resolve(requiredArgument(parsed, "tarballs"));

for (const definition of packageDefinitions) {
  const source = join(stagedPackageDirectory(stage, definition), "package.json");
  const destination = join(tarballs, `${definition.name}-package.json`);
  writeFileSync(destination, readFileSync(source));
}
