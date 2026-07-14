import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));

export const npmRoot = resolve(scriptsDirectory, "..");
export const repositoryRoot = resolve(npmRoot, "..");

export const packageDefinitions = Object.freeze([
  {
    name: "lexa-index-darwin-arm64",
    directory: "darwin-arm64",
    archiveBase: "lexa-macos-apple-silicon",
    archiveExtension: "tar.gz",
    archiveBinary: (version) =>
      `lexa-macos-apple-silicon-${version}/lexa`,
    packageBinary: "bin/lexa",
    os: ["darwin"],
    cpu: ["arm64"],
  },
  {
    name: "lexa-index-darwin-x64",
    directory: "darwin-x64",
    archiveBase: "lexa-macos-intel",
    archiveExtension: "tar.gz",
    archiveBinary: (version) => `lexa-macos-intel-${version}/lexa`,
    packageBinary: "bin/lexa",
    os: ["darwin"],
    cpu: ["x64"],
  },
  {
    name: "lexa-index-linux-x64",
    directory: "linux-x64",
    archiveBase: "lexa-linux-x86_64",
    archiveExtension: "tar.gz",
    archiveBinary: (version) => `lexa-linux-x86_64-${version}/lexa`,
    packageBinary: "bin/lexa",
    os: ["linux"],
    cpu: ["x64"],
  },
  {
    name: "lexa-index-win32-x64",
    directory: "win32-x64",
    archiveBase: "lexa-windows-x86_64",
    archiveExtension: "zip",
    archiveBinary: () => "lexa.exe",
    packageBinary: "bin/lexa.exe",
    os: ["win32"],
    cpu: ["x64"],
  },
  {
    name: "lexa-index",
    directory: "lexa-index",
    wrapper: true,
  },
]);

export const platformDefinitions = packageDefinitions.filter(
  (definition) => !definition.wrapper,
);
export const wrapperDefinition = packageDefinitions.at(-1);

export function sourcePackageDirectory(definition) {
  return join(npmRoot, definition.directory);
}

export function stagedPackageDirectory(stageRoot, definition) {
  return join(stageRoot, definition.directory);
}

export function archiveName(definition, version) {
  return `${definition.archiveBase}-${version}.${definition.archiveExtension}`;
}

export function tarballName(definition, version) {
  return `${definition.name}-${version}.tgz`;
}

export function expectedTarballFiles(definition) {
  if (!definition.wrapper) {
    return ["LICENSE", "README.md", definition.packageBinary, "package.json"];
  }
  return [
    "LICENSE",
    "README.md",
    "bin/lexa.js",
    "index.js",
    "lib/launch.js",
    "lib/resolve-binary.js",
    "lib/version.js",
    "package.json",
    "skill.js",
    "skill/SKILL.md",
  ];
}
