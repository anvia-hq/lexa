import { spawn } from "node:child_process";

const SIGNALS =
  process.platform === "win32"
    ? ["SIGINT", "SIGTERM", "SIGBREAK"]
    : ["SIGHUP", "SIGINT", "SIGTERM"];

const SIGNAL_EXIT_CODES = Object.freeze({
  SIGHUP: 129,
  SIGINT: 130,
  SIGTERM: 143,
  SIGBREAK: 149,
});

export function launch(
  binaryPath,
  args,
  { processRef = process, spawnProcess = spawn } = {},
) {
  return new Promise((resolve, reject) => {
    let child;
    try {
      child = spawnProcess(binaryPath, args, { stdio: "inherit" });
    } catch (error) {
      reject(error);
      return;
    }

    const handlers = new Map();
    for (const signal of SIGNALS) {
      const handler = () => {
        if (child.exitCode === null && child.signalCode === null) {
          try {
            child.kill(signal);
          } catch {
            // The child may have exited between the state check and kill call.
          }
        }
      };
      handlers.set(signal, handler);
      processRef.on(signal, handler);
    }

    const cleanup = () => {
      for (const [signal, handler] of handlers) {
        processRef.off(signal, handler);
      }
    };

    child.once("error", (error) => {
      cleanup();
      reject(error);
    });
    child.once("exit", (code, signal) => {
      cleanup();
      resolve({ code, signal });
    });
  });
}

export function exitLikeChild({ code, signal }, processRef = process) {
  if (code !== null) {
    processRef.exit(code);
    return;
  }

  if (signal !== null && processRef.platform !== "win32") {
    try {
      processRef.kill(processRef.pid, signal);
      return;
    } catch {
      // Fall back to the conventional signal-derived exit status.
    }
  }

  processRef.exit(SIGNAL_EXIT_CODES[signal] ?? 1);
}
