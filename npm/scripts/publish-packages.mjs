import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";

import { parseArguments, requiredArgument } from "./arguments.mjs";
import {
  expectedTarballFiles,
  packageDefinitions,
  tarballName,
} from "./package-definitions.mjs";
import { cargoLexaVersion } from "./cargo-version.mjs";

const REGISTRY = "https://registry.npmjs.org";
const EXPECTED_REPOSITORY = "https://github.com/anvia-hq/lexa";

function normalizeRepository(value) {
  const url = typeof value === "string" ? value : value?.url;
  return String(url ?? "")
    .replace(/^git\+/u, "")
    .replace(/\.git$/u, "")
    .replace(/\/$/u, "");
}

function maintainerNames(document) {
  return (document.maintainers ?? []).map((maintainer) => {
    if (typeof maintainer === "string") {
      return maintainer.split(/\s|</u, 1)[0];
    }
    return maintainer.name;
  });
}

async function registryDocument(name) {
  const response = await fetch(`${REGISTRY}/${encodeURIComponent(name)}`, {
    headers: { accept: "application/json" },
  });
  if (response.status === 404) return null;
  if (!response.ok) {
    throw new Error(`npm registry returned ${response.status} for ${name}`);
  }
  return response.json();
}

function compareJson(label, actual, expected) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} differs: ${JSON.stringify(actual)} != ${JSON.stringify(expected)}`,
    );
  }
}

function validateManifest(definition, manifest, version) {
  compareJson(`${definition.name} name`, manifest.name, definition.name);
  compareJson(`${definition.name} version`, manifest.version, version);
  compareJson(`${definition.name} license`, manifest.license, "MIT");
  compareJson(
    `${definition.name} repository`,
    normalizeRepository(manifest.repository),
    EXPECTED_REPOSITORY,
  );

  if (definition.wrapper) {
    compareJson(`${definition.name} engines`, manifest.engines, { node: ">=20" });
    compareJson(`${definition.name} bin`, manifest.bin, { lexa: "bin/lexa.js" });
    const exactOptionalDependencies = Object.fromEntries(
      packageDefinitions
        .filter((candidate) => !candidate.wrapper)
        .map((candidate) => [candidate.name, version]),
    );
    compareJson(
      `${definition.name} optionalDependencies`,
      manifest.optionalDependencies,
      exactOptionalDependencies,
    );
  } else {
    compareJson(`${definition.name} os`, manifest.os, definition.os);
    compareJson(`${definition.name} cpu`, manifest.cpu, definition.cpu);
    compareJson(`${definition.name} exports`, manifest.exports, {
      "./binary": `./${definition.packageBinary}`,
    });
  }
}

function inspectPackedFiles(definition, specifier) {
  const directory = mkdtempSync(join(tmpdir(), "lexa-npm-registry-"));
  try {
    const result = spawnSync(
      "npm",
      ["pack", specifier, "--pack-destination", directory, "--json"],
      { encoding: "utf8", maxBuffer: 10 * 1024 * 1024 },
    );
    if (result.status !== 0) {
      throw new Error(`failed to inspect ${specifier}: ${result.stderr || result.stdout}`);
    }
    const [report] = JSON.parse(result.stdout);
    const actual = report.files.map(({ path }) => path).toSorted();
    const expected = expectedTarballFiles(definition).toSorted();
    compareJson(`${specifier} tarball files`, actual, expected);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function runPublish(tarball, tag, dryRun) {
  const args = ["publish", tarball, "--access", "public", "--tag", tag];
  if (dryRun) args.push("--dry-run");
  const result = spawnSync("npm", args, { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`npm publish failed for ${tarball}`);
  }
}

async function preflight(definition, owner, version) {
  const document = await registryDocument(definition.name);
  if (document === null) {
    return { exists: false, versionExists: false };
  }

  const maintainers = maintainerNames(document);
  if (!maintainers.includes(owner)) {
    throw new Error(
      `${definition.name} is registered by another owner (${maintainers.join(", ") || "unknown"})`,
    );
  }
  if (normalizeRepository(document.repository) !== EXPECTED_REPOSITORY) {
    throw new Error(`${definition.name} does not belong to ${EXPECTED_REPOSITORY}`);
  }

  const manifest = document.versions?.[version];
  if (manifest === undefined) {
    return { exists: true, versionExists: false };
  }
  validateManifest(definition, manifest, version);
  inspectPackedFiles(definition, `${definition.name}@${version}`);
  return { exists: true, versionExists: true };
}

async function verifyPublished(definitions, owner, version) {
  let lastError;
  for (let attempt = 1; attempt <= 10; attempt += 1) {
    try {
      for (const definition of definitions) {
        const state = await preflight(definition, owner, version);
        if (!state.versionExists) {
          throw new Error(`${definition.name}@${version} is not visible on npm`);
        }
      }
      return;
    } catch (error) {
      lastError = error;
      if (attempt < 10) {
        await new Promise((resolvePromise) => setTimeout(resolvePromise, 3000));
      }
    }
  }
  throw lastError;
}

async function main() {
  const parsed = parseArguments(
    process.argv.slice(2),
    new Set(["--tarballs", "--mode", "--expected-owner"]),
  );
  const tarballs = resolve(requiredArgument(parsed, "tarballs"));
  const mode = requiredArgument(parsed, "mode");
  const owner = requiredArgument(parsed, "expected-owner");
  if (mode !== "dry-run" && mode !== "publish") {
    throw new Error("--mode must be dry-run or publish");
  }

  const version = cargoLexaVersion();
  const reports = JSON.parse(readFileSync(join(tarballs, "pack-report.json"), "utf8"));
  const states = new Map();

  for (const definition of packageDefinitions) {
    const report = reports.find(({ name }) => name === definition.name);
    if (report?.version !== version) {
      throw new Error(`${definition.name} tarball report does not match ${version}`);
    }
    const manifest = JSON.parse(
      readFileSync(join(tarballs, `${definition.name}-package.json`), "utf8"),
    );
    validateManifest(definition, manifest, version);
  }

  // Complete the ownership/availability preflight for all five names before
  // the first registry mutation.
  for (const definition of packageDefinitions) {
    states.set(
      definition.name,
      await preflight(definition, owner, version),
    );
  }

  for (const definition of packageDefinitions) {
    const state = states.get(definition.name);
    if (state.versionExists) {
      console.log(`${definition.name}@${version} already exists and was verified.`);
      continue;
    }
    const tarball = join(tarballs, tarballName(definition, version));
    runPublish(tarball, definition.wrapper ? "latest" : "lexa-native", mode === "dry-run");
  }

  if (mode === "publish") {
    await verifyPublished(packageDefinitions, owner, version);
    console.log(`Verified all five Lexa npm packages at ${version}.`);
  } else {
    console.log("npm publishing dry run completed without registry mutation.");
  }
}

await main();
