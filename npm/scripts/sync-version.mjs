import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { cargoLexaVersion } from "./cargo-version.mjs";
import {
  packageDefinitions,
  platformDefinitions,
  sourcePackageDirectory,
  wrapperDefinition,
} from "./package-definitions.mjs";

function readManifest(directory) {
  const path = join(directory, "package.json");
  return { path, manifest: JSON.parse(readFileSync(path, "utf8")) };
}

function formatManifest(manifest) {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

export function synchronizeVersions({ write = false, packageRoot } = {}) {
  const version = cargoLexaVersion();
  const drift = [];

  for (const definition of packageDefinitions) {
    const directory = packageRoot
      ? join(packageRoot, definition.directory)
      : sourcePackageDirectory(definition);
    const { path, manifest } = readManifest(directory);
    if (manifest.version !== version) {
      drift.push(`${manifest.name}: version ${manifest.version} != ${version}`);
      manifest.version = version;
    }

    if (definition === wrapperDefinition) {
      manifest.optionalDependencies ??= {};
      for (const platform of platformDefinitions) {
        const actual = manifest.optionalDependencies[platform.name];
        if (actual !== version) {
          drift.push(
            `${manifest.name}: optional dependency ${platform.name}@${actual} != ${version}`,
          );
          manifest.optionalDependencies[platform.name] = version;
        }
      }
      const expectedNames = new Set(platformDefinitions.map(({ name }) => name));
      for (const dependency of Object.keys(manifest.optionalDependencies)) {
        if (!expectedNames.has(dependency)) {
          drift.push(`${manifest.name}: unexpected optional dependency ${dependency}`);
        }
      }
    }

    if (write) {
      writeFileSync(path, formatManifest(manifest));
    }
  }

  if (!write && drift.length > 0) {
    throw new Error(`npm package versions are out of sync:\n${drift.join("\n")}`);
  }
  return { version, drift };
}

function main() {
  const mode = process.argv[2];
  if (mode !== "--check" && mode !== "--write") {
    throw new Error("usage: node scripts/sync-version.mjs <--check|--write>");
  }
  const { version, drift } = synchronizeVersions({ write: mode === "--write" });
  if (mode === "--write" && drift.length > 0) {
    console.log(`Synchronized npm package versions to ${version}.`);
  } else {
    console.log(`All npm package versions match Lexa ${version}.`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
