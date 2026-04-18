const fs = require("fs");
const path = require("path");
const os = require("os");
const http = require("http");
const { spawn, spawnSync } = require("child_process");

const repoRoot = path.resolve(__dirname, "..", "..");
const targetDir = path.join(repoRoot, "target", "playwright-e2e");
const statePath = path.join(targetDir, "state.json");
const profileDir = path.join(targetDir, "profile");
const inboxDir = path.join(targetDir, "fixtures", "inbox");
const attachmentDir = path.join(targetDir, "fixtures", "attachments");
const attachmentPath = path.join(attachmentDir, "reference.png");
const dashboardRoot = path.join(repoRoot, "ui", "dashboard");
const dashboardBundlePath = path.join(repoRoot, "crates", "agent-daemon", "static-modern", "index.html");
const pluginFixtureDir = path.join(repoRoot, "tests", "dashboard-e2e", "fixtures", "echo-plugin");
const pluginSourceDir = path.join(targetDir, "fixtures", "echo-plugin");
const installDir = path.join(targetDir, "install");
const daemonPort = 42791;
const providerPort = 42792;
const releasePort = 42793;
const daemonToken = "playwright-daemon-token";
const windowsExecutables = [
  path.join(repoRoot, "target", "debug", "nuclear.exe"),
];
const unixExecutables = [
  path.join(repoRoot, "target", "debug", "nuclear"),
];
const rebuildExtensions = new Set([".rs", ".html", ".css", ".js", ".cjs", ".toml", ".lock", ".ts", ".tsx"]);
const currentVersion = "0.8.2";
const candidateVersion = "0.8.3";

let mockServer = null;
let releaseServer = null;
let daemonChild = null;
let shuttingDown = false;

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function removeDirWithRetry(dir, attempts = 8, delayMs = 350) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      fs.rmSync(dir, { recursive: true, force: true });
      return;
    } catch (error) {
      if (attempt === attempts || !["ENOTEMPTY", "EBUSY", "EPERM"].includes(error.code)) {
        throw error;
      }
      await sleep(delayMs);
    }
  }
}

function copyDirRecursive(source, target) {
  ensureDir(target);
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      copyDirRecursive(sourcePath, targetPath);
    } else if (entry.isFile()) {
      fs.copyFileSync(sourcePath, targetPath);
    }
  }
}

function makeEnv() {
  const appData = path.join(profileDir, "AppData", "Roaming");
  const localAppData = path.join(profileDir, "AppData", "Local");
  ensureDir(appData);
  ensureDir(localAppData);
  ensureDir(inboxDir);
  ensureDir(attachmentDir);
  return {
    ...process.env,
    HOME: profileDir,
    USERPROFILE: profileDir,
    APPDATA: appData,
    LOCALAPPDATA: localAppData,
    NUCLEAR_UPDATE_RELEASES_URL: `http://127.0.0.1:${releasePort}/releases/latest`,
  };
}

function executablePath() {
  const candidates = process.platform === "win32" ? windowsExecutables : unixExecutables;
  return candidates.find((candidate) => fs.existsSync(candidate)) || candidates[0];
}

function managedExecutableName() {
  return process.platform === "win32" ? "nuclear.exe" : "nuclear";
}

function newestMtimeMs(entryPath) {
  if (!fs.existsSync(entryPath)) {
    return 0;
  }
  const stats = fs.statSync(entryPath);
  if (stats.isFile()) {
    return rebuildExtensions.has(path.extname(entryPath).toLowerCase()) ? stats.mtimeMs : 0;
  }
  if (!stats.isDirectory()) {
    return 0;
  }
  let newest = 0;
  for (const entry of fs.readdirSync(entryPath, { withFileTypes: true })) {
    newest = Math.max(newest, newestMtimeMs(path.join(entryPath, entry.name)));
  }
  return newest;
}

function binaryNeedsRebuild(exe) {
  if (!fs.existsSync(exe)) {
    return true;
  }
  const binaryMtimeMs = fs.statSync(exe).mtimeMs;
  const newestSourceMtimeMs = Math.max(
    newestMtimeMs(path.join(repoRoot, "Cargo.toml")),
    newestMtimeMs(path.join(repoRoot, "Cargo.lock")),
    newestMtimeMs(path.join(repoRoot, "crates")),
    newestMtimeMs(path.join(repoRoot, "tests", "dashboard-e2e"))
  );
  return newestSourceMtimeMs > binaryMtimeMs;
}

function dashboardNeedsBuild() {
  if (!fs.existsSync(dashboardBundlePath)) {
    return true;
  }
  const bundleMtimeMs = fs.statSync(dashboardBundlePath).mtimeMs;
  const newestSourceMtimeMs = Math.max(
    newestMtimeMs(path.join(dashboardRoot, "package.json")),
    newestMtimeMs(path.join(dashboardRoot, "package-lock.json")),
    newestMtimeMs(path.join(dashboardRoot, "tsconfig.json")),
    newestMtimeMs(path.join(dashboardRoot, "vite.config.ts")),
    newestMtimeMs(path.join(dashboardRoot, "src"))
  );
  return newestSourceMtimeMs > bundleMtimeMs;
}

function ensureDashboardBuilt() {
  const nodeModulesPath = path.join(dashboardRoot, "node_modules");
  if (!fs.existsSync(nodeModulesPath)) {
    const install = spawnSync("npm", ["ci"], {
      cwd: dashboardRoot,
      stdio: "inherit",
      shell: true,
    });
    if (install.status !== 0) {
      throw new Error("failed to install ui/dashboard dependencies for dashboard e2e");
    }
  }

  if (dashboardNeedsBuild()) {
    const build = spawnSync("npm", ["run", "build"], {
      cwd: dashboardRoot,
      stdio: "inherit",
      shell: true,
    });
    if (build.status !== 0) {
      throw new Error("failed to build ui/dashboard for dashboard e2e");
    }
  }
}

function writeAttachmentFixture() {
  ensureDir(attachmentDir);
  fs.writeFileSync(
    attachmentPath,
    Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAukB9pVHtV8AAAAASUVORK5CYII=",
      "base64"
    )
  );
}

function ensureBinaryBuilt() {
  const exe = executablePath();
  if (binaryNeedsRebuild(exe)) {
    const build = spawnSync("cargo", ["build", "-q", "-p", "nuclear", "--bin", "nuclear"], {
      cwd: repoRoot,
      stdio: "inherit",
    });
    if (build.status !== 0) {
      throw new Error("failed to build the nuclear package for dashboard e2e");
    }
  }
  if (!fs.existsSync(exe)) {
    throw new Error(`expected built binary at ${exe}`);
  }
  return exe;
}

function prepareManagedExecutable(exe) {
  ensureDir(installDir);
  const managedExe = path.join(installDir, managedExecutableName());
  fs.copyFileSync(exe, managedExe);
  if (process.platform !== "win32") {
    fs.chmodSync(managedExe, 0o755);
  }
  fs.writeFileSync(
    path.join(installDir, "install-state.json"),
    JSON.stringify(
      {
        schema_version: 1,
        display_name: "Nuclear Agent",
        command_name: "nuclear",
        install_dir: installDir,
        installed_at: new Date().toISOString(),
        version: currentVersion,
        install_source: "bundled",
        rollback_binary: null,
        previous_binary_source: null,
      },
      null,
      2
    )
  );
  return managedExe;
}

function discoverConfigPath(exe, env) {
  const doctor = spawnSync(exe, ["doctor"], {
    cwd: repoRoot,
    env,
    encoding: "utf8",
  });
  if (doctor.status !== 0) {
    throw new Error(`failed to discover config path:\n${doctor.stderr || doctor.stdout}`);
  }
  const match = doctor.stdout.match(/^config_path=(.+)$/m);
  if (!match) {
    throw new Error(`doctor output did not include config_path:\n${doctor.stdout}`);
  }
  return match[1].trim();
}

function writeE2EConfig(configPath) {
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  config.onboarding_complete = true;
  config.daemon.host = "127.0.0.1";
  config.daemon.port = daemonPort;
  config.daemon.token = daemonToken;
  config.providers = [
    {
      id: "local-codex",
      display_name: "Local Codex",
      kind: "open_ai_compatible",
      base_url: `http://127.0.0.1:${providerPort}/v1`,
      auth_mode: "none",
      default_model: "mock-codex",
      keychain_account: null,
      oauth: null,
      local: true,
    },
    {
      id: "local-claude",
      display_name: "Local Claude",
      kind: "open_ai_compatible",
      base_url: `http://127.0.0.1:${providerPort}/v1`,
      auth_mode: "none",
      default_model: "mock-claude",
      keychain_account: null,
      oauth: null,
      local: true,
    },
  ];
  config.aliases = [
    {
      alias: "main",
      provider_id: "local-codex",
      model: "mock-codex",
      description: "Primary local coding alias",
    },
    {
      alias: "claude",
      provider_id: "local-claude",
      model: "mock-claude",
      description: "Secondary local reasoning alias",
    },
  ];
  config.main_agent_alias = "main";
  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
  ensureDir(targetDir);
  fs.writeFileSync(
    statePath,
    JSON.stringify(
      {
        baseURL: `http://127.0.0.1:${daemonPort}`,
        token: daemonToken,
        inboxPath: inboxDir,
        pluginPath: pluginSourceDir,
        attachmentPath,
      },
      null,
      2
    )
  );
}

function startMockProviderServer() {
  mockServer = http.createServer(async (req, res) => {
    const requestUrl = new URL(req.url, `http://${req.headers.host}`);
    if (req.method === "GET" && requestUrl.pathname === "/v1/models") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          data: [{ id: "mock-codex" }, { id: "mock-claude" }],
        })
      );
      return;
    }

    if (req.method === "POST" && requestUrl.pathname === "/v1/chat/completions") {
      const chunks = [];
      for await (const chunk of req) {
        chunks.push(chunk);
      }
      const raw = Buffer.concat(chunks).toString("utf8");
      const body = raw ? JSON.parse(raw) : {};
      const messages = Array.isArray(body.messages) ? body.messages : [];
      const lastUser = [...messages].reverse().find((message) => message.role === "user");
      const prompt = typeof lastUser?.content === "string" ? lastUser.content : "empty";
      const model = body.model || "mock-model";
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          id: "chatcmpl-mock",
          object: "chat.completion",
          created: Math.floor(Date.now() / 1000),
          model,
          choices: [
            {
              index: 0,
              message: {
                role: "assistant",
                content: `Mock reply from ${model}: ${prompt}`,
              },
              finish_reason: "stop",
            },
          ],
        })
      );
      return;
    }

    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "not found" }));
  });

  return new Promise((resolve, reject) => {
    mockServer.once("error", reject);
    mockServer.listen(providerPort, "127.0.0.1", () => resolve());
  });
}

function startReleaseServer() {
  releaseServer = http.createServer((req, res) => {
    const requestUrl = new URL(req.url, `http://${req.headers.host}`);
    if (req.method === "GET" && requestUrl.pathname === "/releases/latest") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          tag_name: `v${candidateVersion}`,
          draft: false,
          prerelease: false,
          published_at: "2026-04-17T00:00:00Z",
          assets: [
            {
              name:
                process.platform === "win32"
                  ? `nuclear-${candidateVersion}-windows-x64-full.zip`
                  : `nuclear-${candidateVersion}-linux-x64-full.tar.gz`,
              browser_download_url: `http://127.0.0.1:${releasePort}/downloads/archive`,
            },
            {
              name:
                process.platform === "win32"
                  ? `nuclear-${candidateVersion}-windows-x64-full.zip.sha256.txt`
                  : `nuclear-${candidateVersion}-linux-x64-full.tar.gz.sha256.txt`,
              browser_download_url: `http://127.0.0.1:${releasePort}/downloads/checksum`,
            },
          ],
        })
      );
      return;
    }

    res.writeHead(200, { "Content-Type": "text/plain" });
    res.end("unused");
  });

  return new Promise((resolve, reject) => {
    releaseServer.once("error", reject);
    releaseServer.listen(releasePort, "127.0.0.1", () => resolve());
  });
}

function startDaemon(exe, env) {
  daemonChild = spawn(exe, ["__daemon"], {
    cwd: repoRoot,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  daemonChild.stdout.on("data", (chunk) => {
    process.stdout.write(`[dashboard-e2e daemon] ${chunk}`);
  });
  daemonChild.stderr.on("data", (chunk) => {
    process.stderr.write(`[dashboard-e2e daemon] ${chunk}`);
  });
  daemonChild.on("exit", (code, signal) => {
    if (!shuttingDown) {
      console.error(`dashboard e2e daemon exited unexpectedly (code=${code}, signal=${signal})`);
      process.exit(code || 1);
    }
  });
}

async function waitForDaemonReady() {
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${daemonPort}/v1/status`, {
        headers: { Authorization: `Bearer ${daemonToken}` },
      });
      if (response.ok) {
        return;
      }
    } catch (_) {
      // retry
    }
    await sleep(500);
  }
  throw new Error("dashboard e2e daemon did not become ready in time");
}

async function cleanupAndExit(code) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  if (daemonChild && !daemonChild.killed) {
    daemonChild.kill("SIGTERM");
  }
  if (mockServer) {
    await new Promise((resolve) => mockServer.close(resolve));
  }
  if (releaseServer) {
    await new Promise((resolve) => releaseServer.close(resolve));
  }
  process.exit(code);
}

async function main() {
  await removeDirWithRetry(targetDir);
  ensureDir(targetDir);
  copyDirRecursive(pluginFixtureDir, pluginSourceDir);
  writeAttachmentFixture();
  ensureDashboardBuilt();
  const env = makeEnv();
  const builtExe = ensureBinaryBuilt();
  const exe = prepareManagedExecutable(builtExe);
  const configPath = discoverConfigPath(exe, env);
  writeE2EConfig(configPath);
  await startMockProviderServer();
  await startReleaseServer();
  startDaemon(exe, env);
  await waitForDaemonReady();
  process.stdout.write("[dashboard-e2e] ready\n");
}

process.on("SIGINT", () => {
  cleanupAndExit(0).catch((error) => {
    console.error(error);
    process.exit(1);
  });
});
process.on("SIGTERM", () => {
  cleanupAndExit(0).catch((error) => {
    console.error(error);
    process.exit(1);
  });
});

main()
  .then(() => {
    process.stdin.resume();
  })
  .catch((error) => {
    console.error(error);
    cleanupAndExit(1).catch(() => process.exit(1));
  });
