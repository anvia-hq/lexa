import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

import { cargoLexaVersion } from "../scripts/cargo-version.mjs";
import {
  expectedTarballFiles,
  npmRoot,
  packageDefinitions,
  platformDefinitions,
  repositoryRoot,
} from "../scripts/package-definitions.mjs";
import { synchronizeVersions } from "../scripts/sync-version.mjs";
import { lexaSkill, lexaVersion } from "../lexa-index/skill.js";

function manifest(definition) {
  return JSON.parse(
    readFileSync(join(npmRoot, definition.directory, "package.json"), "utf8"),
  );
}

test("all package versions and optional dependencies exactly match Cargo", () => {
  const version = cargoLexaVersion();
  assert.doesNotThrow(() => synchronizeVersions());
  for (const definition of packageDefinitions) {
    assert.equal(manifest(definition).version, version);
  }
  assert.equal(lexaVersion, version);
});

test("packaged SKILL.md matches the repository source byte-for-byte", () => {
  const source = readFileSync(join(repositoryRoot, "skill", "SKILL.md"));
  const packaged = readFileSync(join(npmRoot, "lexa-index", "skill", "SKILL.md"));
  assert.deepEqual(packaged, source);
  assert.equal(lexaSkill, source.toString("utf8"));
});

test("all packages contain the repository MIT license byte-for-byte", () => {
  const source = readFileSync(join(repositoryRoot, "LICENSE"));
  for (const definition of packageDefinitions) {
    const packaged = readFileSync(join(npmRoot, definition.directory, "LICENSE"));
    assert.deepEqual(packaged, source);
  }
});

test("platform packages have exact restrictions and no lifecycle scripts", () => {
  for (const definition of platformDefinitions) {
    const packageManifest = manifest(definition);
    assert.deepEqual(packageManifest.os, definition.os);
    assert.deepEqual(packageManifest.cpu, definition.cpu);
    assert.equal(
      packageManifest.exports["./binary"],
      `./${definition.packageBinary}`,
    );
    assert.equal(packageManifest.scripts, undefined);
    assert.deepEqual(
      packageManifest.files.toSorted(),
      expectedTarballFiles(definition)
        .filter((path) => path !== "package.json")
        .toSorted(),
    );
  }
});

test("wrapper uses exact optional versions and no install scripts", () => {
  const wrapper = manifest(packageDefinitions.at(-1));
  for (const definition of platformDefinitions) {
    assert.equal(wrapper.optionalDependencies[definition.name], wrapper.version);
  }
  assert.equal(wrapper.scripts, undefined);
  assert.equal(wrapper.engines.node, ">=20");
  assert.deepEqual(wrapper.bin, { lexa: "bin/lexa.js" });
});

test("published runtime JavaScript contains no network client", () => {
  const files = [
    "index.js",
    "skill.js",
    "bin/lexa.js",
    "lib/launch.js",
    "lib/resolve-binary.js",
    "lib/version.js",
  ];
  for (const file of files) {
    const source = readFileSync(join(npmRoot, "lexa-index", file), "utf8");
    assert.doesNotMatch(source, /node:(?:http|https|net|tls)|\bfetch\s*\(/u);
    assert.doesNotMatch(source, /github\.com|registry\.npmjs\.org/u);
  }
});
