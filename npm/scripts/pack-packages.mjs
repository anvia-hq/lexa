import {
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";

import { parseArguments, requiredArgument } from "./arguments.mjs";
import {
  expectedTarballFiles,
  packageDefinitions,
  stagedPackageDirectory,
} from "./package-definitions.mjs";

function npmPack(directory, args) {
  const result = spawnSync("npm", ["pack", ...args, "--json"], {
    cwd: directory,
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`npm pack failed in ${directory}: ${result.stderr || result.stdout}`);
  }
  const report = JSON.parse(result.stdout);
  if (!Array.isArray(report) || report.length !== 1) {
    throw new Error(`unexpected npm pack report in ${directory}`);
  }
  return report[0];
}

function inspectReport(definition, report) {
  const expected = expectedTarballFiles(definition).toSorted();
  const actual = report.files.map(({ path }) => path).toSorted();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${definition.name} tarball files differ:\nexpected ${expected.join(", ")}\n` +
        `actual ${actual.join(", ")}`,
    );
  }

  const manifest = report.files.find(({ path }) => path === "package.json");
  if (manifest === undefined) {
    throw new Error(`${definition.name} tarball omitted package.json`);
  }
  const binary = report.files.find(
    ({ path }) => path === (definition.packageBinary ?? "bin/lexa.js"),
  );
  if (binary === undefined) {
    throw new Error(`${definition.name} tarball omitted its executable`);
  }
  if (definition.packageBinary === "bin/lexa" && binary.mode !== 0o755) {
    throw new Error(`${definition.name} binary mode is ${binary.mode}, expected 0755`);
  }
}

function main() {
  const parsed = parseArguments(
    process.argv.slice(2),
    new Set(["--stage", "--output"]),
  );
  const stage = resolve(requiredArgument(parsed, "stage"));
  const output = resolve(requiredArgument(parsed, "output"));
  rmSync(output, { recursive: true, force: true });
  mkdirSync(output, { recursive: true });

  const reports = [];
  for (const definition of packageDefinitions) {
    const directory = stagedPackageDirectory(stage, definition);
    const manifest = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
    if (manifest.scripts?.install !== undefined || manifest.scripts?.postinstall !== undefined) {
      throw new Error(`${definition.name} must not define install or postinstall scripts`);
    }

    const dryRun = npmPack(directory, ["--dry-run"]);
    inspectReport(definition, dryRun);
    const packed = npmPack(directory, ["--pack-destination", output]);
    inspectReport(definition, packed);
    reports.push({
      name: definition.name,
      version: packed.version,
      filename: packed.filename,
      size: packed.size,
      unpackedSize: packed.unpackedSize,
      shasum: packed.shasum,
      integrity: packed.integrity,
      files: packed.files,
    });
  }

  writeFileSync(join(output, "pack-report.json"), `${JSON.stringify(reports, null, 2)}\n`);
  console.log(JSON.stringify(reports, null, 2));
}

main();
