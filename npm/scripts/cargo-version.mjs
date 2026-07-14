import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

import { repositoryRoot } from "./package-definitions.mjs";

export function cargoLexaVersion() {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--locked"],
    {
      cwd: repositoryRoot,
      encoding: "utf8",
      maxBuffer: 10 * 1024 * 1024,
    },
  );
  if (result.status !== 0) {
    throw new Error(
      `cargo metadata failed: ${result.stderr || result.stdout || result.error}`,
    );
  }

  const metadata = JSON.parse(result.stdout);
  const rootManifest = resolve(repositoryRoot, "Cargo.toml");
  const packageMetadata = metadata.packages.find(
    (candidate) =>
      candidate.name === "lexa" && resolve(candidate.manifest_path) === rootManifest,
  );
  if (packageMetadata === undefined) {
    throw new Error("cargo metadata did not contain the root Lexa package");
  }
  return packageMetadata.version;
}
