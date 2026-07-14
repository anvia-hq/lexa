#!/usr/bin/env node

import { launch, exitLikeChild } from "../lib/launch.js";

try {
  const { binaryPath } = await import("../index.js");
  const result = await launch(binaryPath, process.argv.slice(2));
  exitLikeChild(result);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`lexa: ${message}`);
  process.exit(1);
}
