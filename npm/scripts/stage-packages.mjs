import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";

import { parseArguments, requiredArgument } from "./arguments.mjs";
import {
  archiveName,
  packageDefinitions,
  platformDefinitions,
  repositoryRoot,
  sourcePackageDirectory,
  stagedPackageDirectory,
  wrapperDefinition,
} from "./package-definitions.mjs";
import { synchronizeVersions } from "./sync-version.mjs";

function checksums(path) {
  const entries = new Map();
  for (const line of readFileSync(path, "utf8").split(/\r?\n/u)) {
    if (line.trim().length === 0) continue;
    const match = /^([a-fA-F0-9]{64})\s+\*?(.+)$/u.exec(line);
    if (match === null) {
      throw new Error(`invalid checksum line: ${line}`);
    }
    entries.set(match[2], match[1].toLowerCase());
  }
  return entries;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function extractBinary(definition, version, archivePath, destination) {
  let command;
  let args;
  if (definition.archiveExtension === "tar.gz") {
    command = "tar";
    args = ["-xOzf", archivePath, definition.archiveBinary(version)];
  } else {
    command = "unzip";
    args = ["-p", archivePath, definition.archiveBinary(version)];
  }
  const result = spawnSync(command, args, {
    encoding: null,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(
      `failed to extract ${definition.archiveBinary(version)} from ${archivePath}: ` +
        `${result.stderr?.toString() || result.error}`,
    );
  }
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, result.stdout);
  if (definition.archiveExtension === "tar.gz") {
    chmodSync(destination, 0o755);
  }
}

function main() {
  const parsed = parseArguments(
    process.argv.slice(2),
    new Set(["--release-assets", "--output"]),
  );
  const releaseAssets = resolve(requiredArgument(parsed, "release-assets"));
  const output = resolve(requiredArgument(parsed, "output"));
  const { version } = synchronizeVersions();

  const sourceSkill = readFileSync(join(repositoryRoot, "skill", "SKILL.md"));
  const packagedSkill = readFileSync(
    join(sourcePackageDirectory(wrapperDefinition), "skill", "SKILL.md"),
  );
  if (!sourceSkill.equals(packagedSkill)) {
    throw new Error("npm/lexa-index/skill/SKILL.md differs from skill/SKILL.md");
  }

  const checksumPath = join(releaseAssets, "SHA256SUMS");
  if (!existsSync(checksumPath)) {
    throw new Error(`missing checksum file: ${checksumPath}`);
  }
  const expectedChecksums = checksums(checksumPath);
  for (const definition of platformDefinitions) {
    const filename = archiveName(definition, version);
    const archivePath = join(releaseAssets, filename);
    if (!existsSync(archivePath)) {
      throw new Error(`missing release archive: ${archivePath}`);
    }
    const expected = expectedChecksums.get(filename);
    if (expected === undefined) {
      throw new Error(`SHA256SUMS does not contain ${filename}`);
    }
    const actual = sha256(archivePath);
    if (actual !== expected) {
      throw new Error(`checksum mismatch for ${filename}: ${actual} != ${expected}`);
    }
  }

  rmSync(output, { recursive: true, force: true });
  mkdirSync(output, { recursive: true });
  for (const definition of packageDefinitions) {
    cpSync(
      sourcePackageDirectory(definition),
      stagedPackageDirectory(output, definition),
      { recursive: true },
    );
  }

  writeFileSync(
    join(stagedPackageDirectory(output, wrapperDefinition), "skill", "SKILL.md"),
    sourceSkill,
  );
  synchronizeVersions({ write: true, packageRoot: output });

  for (const definition of platformDefinitions) {
    const archivePath = join(releaseAssets, archiveName(definition, version));
    const destination = join(
      stagedPackageDirectory(output, definition),
      definition.packageBinary,
    );
    extractBinary(definition, version, archivePath, destination);
  }

  console.log(`Staged ${packageDefinitions.length} Lexa npm packages at ${output}.`);
}

main();
