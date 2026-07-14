import assert from "node:assert/strict";
import test from "node:test";

import {
  PLATFORM_PACKAGES,
  binarySpecifier,
  resolveBinaryPath,
} from "../lexa-index/lib/resolve-binary.js";

const targets = [
  ["darwin", "arm64", "lexa-index-darwin-arm64/binary"],
  ["darwin", "x64", "lexa-index-darwin-x64/binary"],
  ["linux", "x64", "lexa-index-linux-x64/binary"],
  ["win32", "x64", "lexa-index-win32-x64/binary"],
];

test("maps every supported platform and architecture to its binary export", () => {
  assert.equal(Object.keys(PLATFORM_PACKAGES).length, targets.length);
  for (const [platform, architecture, expected] of targets) {
    assert.equal(binarySpecifier(platform, architecture), expected);
  }
});

test("rejects unsupported targets clearly", () => {
  assert.throws(
    () => binarySpecifier("linux", "arm64"),
    /Lexa does not currently support linux-arm64\./u,
  );
});

test("returns an absolute path from the selected platform package", () => {
  const requested = [];
  const resolved = resolveBinaryPath({
    platform: "win32",
    architecture: "x64",
    resolve(specifier) {
      requested.push(specifier);
      return "/temporary/package/bin/lexa.exe";
    },
  });
  assert.deepEqual(requested, ["lexa-index-win32-x64/binary"]);
  assert.equal(resolved, "/temporary/package/bin/lexa.exe");
});

test("explains missing optional platform packages", () => {
  assert.throws(
    () =>
      resolveBinaryPath({
        platform: "darwin",
        architecture: "arm64",
        resolve() {
          throw new Error("MODULE_NOT_FOUND");
        },
      }),
    /--omit=optional.*optional dependencies enabled/su,
  );
});
