import {
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { spawn, spawnSync } from "node:child_process";
import { delimiter, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";

import { parseArguments, requiredArgument } from "./arguments.mjs";
import {
  packageDefinitions,
  repositoryRoot,
  tarballName,
} from "./package-definitions.mjs";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    shell: process.platform === "win32",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.status}):\n${result.stdout}\n${result.stderr}`,
    );
  }
  return result;
}

function verifyTerminationSignal(executable, directory, environment) {
  if (process.platform === "win32") return Promise.resolve();

  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, ["mcp", directory], {
      cwd: directory,
      env: environment,
      stdio: ["pipe", "ignore", "ignore"],
    });
    const deadline = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("Lexa launcher did not terminate after SIGTERM"));
    }, 5000);
    const terminate = setTimeout(() => child.kill("SIGTERM"), 250);
    child.once("error", (error) => {
      clearTimeout(terminate);
      clearTimeout(deadline);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(terminate);
      clearTimeout(deadline);
      if (signal !== "SIGTERM" && code !== 143) {
        reject(
          new Error(
            `Lexa launcher exited unexpectedly after SIGTERM: code=${code} signal=${signal}`,
          ),
        );
        return;
      }
      resolvePromise();
    });
  });
}

const parsed = parseArguments(
  process.argv.slice(2),
  new Set(["--tarballs", "--platform-package", "--version"]),
);
const tarballs = resolve(requiredArgument(parsed, "tarballs"));
const platformName = requiredArgument(parsed, "platform-package");
const reports = JSON.parse(readFileSync(join(tarballs, "pack-report.json"), "utf8"));
const version =
  parsed.version ?? reports.find(({ name }) => name === "lexa-index")?.version;
if (typeof version !== "string") {
  throw new Error("could not determine the Lexa version from pack-report.json");
}
const platform = packageDefinitions.find(({ name }) => name === platformName);
if (platform === undefined || platform.wrapper) {
  throw new Error(`unknown platform package: ${platformName}`);
}

const directory = mkdtempSync(join(tmpdir(), "lexa-npm-install-"));
try {
  writeFileSync(
    join(directory, "package.json"),
    `${JSON.stringify({ name: "lexa-local-install-test", private: true }, null, 2)}\n`,
  );
  const wrapperTarball = join(
    tarballs,
    tarballName(packageDefinitions.at(-1), version),
  );
  const platformTarball = join(tarballs, tarballName(platform, version));
  run(
    process.platform === "win32" ? "npm.cmd" : "npm",
    [
      "install",
      "--offline",
      "--no-audit",
      "--no-fund",
      wrapperTarball,
      platformTarball,
    ],
    { cwd: directory },
  );

  const environment = {
    ...process.env,
    LEXA_NO_UPDATE_CHECK: "1",
    PATH: `${join(directory, "node_modules", ".bin")}${delimiter}${dirname(process.execPath)}`,
  };
  const executable = join(
    directory,
    "node_modules",
    ".bin",
    process.platform === "win32" ? "lexa.cmd" : "lexa",
  );
  const versionResult = run(executable, ["--version"], {
    cwd: directory,
    env: environment,
  });
  if (!versionResult.stdout.startsWith(`lexa ${version}`)) {
    throw new Error(`unexpected Lexa version output: ${versionResult.stdout}`);
  }
  run(executable, ["status"], { cwd: directory, env: environment });
  await verifyTerminationSignal(executable, directory, environment);

  const inspection = run(
    process.execPath,
    [
      "--input-type=module",
      "--eval",
      [
        'import { binaryPath, lexaVersion } from "lexa-index";',
        'import { lexaSkill } from "lexa-index/skill";',
        "console.log(JSON.stringify({ binaryPath, lexaVersion, lexaSkill }));",
      ].join(" "),
    ],
    { cwd: directory, env: environment },
  );
  const exported = JSON.parse(inspection.stdout);
  const expectedBinaryRoot = join(
    directory,
    "node_modules",
    platformName,
    "bin",
  );
  if (!realpathSync(exported.binaryPath).startsWith(realpathSync(expectedBinaryRoot))) {
    throw new Error(`resolver used an unexpected binary: ${exported.binaryPath}`);
  }
  if (exported.lexaVersion !== version) {
    throw new Error(`skill/version export mismatch: ${exported.lexaVersion}`);
  }
  const sourceSkill = readFileSync(join(repositoryRoot, "skill", "SKILL.md"), "utf8");
  if (exported.lexaSkill !== sourceSkill) {
    throw new Error("installed skill differs from repository skill/SKILL.md");
  }

  const npxEnvironment = {
    ...environment,
    PATH: `${join(directory, "node_modules", ".bin")}${delimiter}${process.env.PATH ?? ""}`,
  };
  const npx = process.platform === "win32" ? "npx.cmd" : "npx";
  const npxResult = run(
    npx,
    [
      "--offline",
      "--package",
      wrapperTarball,
      "--package",
      platformTarball,
      "lexa",
      "--version",
    ],
    {
      cwd: directory,
      env: npxEnvironment,
    },
  );
  if (!npxResult.stdout.startsWith(`lexa ${version}`)) {
    throw new Error(`unexpected npx output: ${npxResult.stdout}`);
  }

  console.log(`Local tarball installation passed for ${platformName}.`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
