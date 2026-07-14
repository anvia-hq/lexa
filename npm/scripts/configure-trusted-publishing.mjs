import { spawnSync } from "node:child_process";

import { packageDefinitions } from "./package-definitions.mjs";

function npm(args, capture = false) {
  const result = spawnSync("npm", args, {
    encoding: capture ? "utf8" : undefined,
    stdio: capture ? "pipe" : "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`npm ${args.join(" ")} failed${capture ? `: ${result.stderr}` : ""}`);
  }
  return result.stdout;
}

const version = npm(["--version"], true).trim().split(".").map(Number);
if (version[0] < 11 || (version[0] === 11 && version[1] < 15)) {
  throw new Error("trusted publishing setup requires npm 11.15.0 or newer");
}

for (const { name } of packageDefinitions) {
  const existing = JSON.parse(npm(["trust", "list", name, "--json"], true));
  if (Array.isArray(existing) ? existing.length > 0 : Object.keys(existing).length > 0) {
    const encoded = JSON.stringify(existing);
    if (!encoded.includes("anvia-hq/lexa") || !encoded.includes("release.yml")) {
      throw new Error(`${name} has an unexpected existing trusted publisher`);
    }
    console.log(`${name} already trusts anvia-hq/lexa release.yml.`);
    continue;
  }
  npm([
    "trust",
    "github",
    name,
    "--file",
    "release.yml",
    "--repo",
    "anvia-hq/lexa",
    "--allow-publish",
    "--yes",
  ]);
}

for (const { name } of packageDefinitions) {
  const trust = npm(["trust", "list", name, "--json"], true);
  if (!trust.includes("anvia-hq/lexa") || !trust.includes("release.yml")) {
    throw new Error(`trusted publisher verification failed for ${name}`);
  }
}
console.log("Verified trusted publishing for all five Lexa packages.");
