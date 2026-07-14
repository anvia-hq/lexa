import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { exitLikeChild, launch } from "../lexa-index/lib/launch.js";

test("forwards original arguments and preserves the native exit code", async () => {
  const directory = mkdtempSync(join(tmpdir(), "lexa-launch-test-"));
  try {
    const helper = join(directory, "helper.mjs");
    const output = join(directory, "args.json");
    writeFileSync(
      helper,
      [
        'import { writeFileSync } from "node:fs";',
        "const [output, ...args] = process.argv.slice(2);",
        "writeFileSync(output, JSON.stringify(args));",
        "process.exit(23);",
      ].join("\n"),
    );
    const args = ["--literal", "two words", "--", "$unchanged", "尾"];
    const result = await launch(process.execPath, [helper, output, ...args]);
    assert.deepEqual(result, { code: 23, signal: null });
    assert.deepEqual(JSON.parse(readFileSync(output, "utf8")), args);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("forwards termination signals to the child process", async () => {
  const directory = mkdtempSync(join(tmpdir(), "lexa-signal-test-"));
  try {
    const helper = join(directory, "wait.mjs");
    writeFileSync(helper, "setInterval(() => {}, 1000);\n");
    const processRef = new EventEmitter();
    const resultPromise = launch(process.execPath, [helper], { processRef });
    setTimeout(() => processRef.emit("SIGTERM"), 100);
    const result = await resultPromise;
    if (process.platform === "win32") {
      assert.ok(result.signal === "SIGTERM" || result.code !== 0);
    } else {
      assert.equal(result.signal, "SIGTERM");
    }
    assert.equal(processRef.listenerCount("SIGTERM"), 0);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("exitLikeChild uses the child's numeric status", () => {
  let exitCode;
  exitLikeChild(
    { code: 17, signal: null },
    {
      exit(code) {
        exitCode = code;
      },
    },
  );
  assert.equal(exitCode, 17);
});
