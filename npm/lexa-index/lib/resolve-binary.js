import { createRequire } from "node:module";
import { resolve as resolvePath } from "node:path";

const require = createRequire(import.meta.url);

export const PLATFORM_PACKAGES = Object.freeze({
  "darwin-arm64": "lexa-index-darwin-arm64",
  "darwin-x64": "lexa-index-darwin-x64",
  "linux-x64": "lexa-index-linux-x64",
  "win32-x64": "lexa-index-win32-x64",
});

export function binarySpecifier(platform = process.platform, architecture = process.arch) {
  const target = `${platform}-${architecture}`;
  const packageName = PLATFORM_PACKAGES[target];
  if (packageName === undefined) {
    throw new Error(`Lexa does not currently support ${target}.`);
  }
  return `${packageName}/binary`;
}

export function resolveBinaryPath({
  platform = process.platform,
  architecture = process.arch,
  resolve = require.resolve,
} = {}) {
  const specifier = binarySpecifier(platform, architecture);
  try {
    return resolvePath(resolve(specifier));
  } catch (cause) {
    const packageName = specifier.slice(0, -"/binary".length);
    throw new Error(
      `Lexa could not load its native package (${packageName}). ` +
        "The installation may have used --omit=optional or a package manager " +
        "configuration that skipped optional dependencies. Reinstall lexa-index " +
        "with optional dependencies enabled.",
      { cause },
    );
  }
}
