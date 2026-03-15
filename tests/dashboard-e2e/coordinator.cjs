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
const daemonPort = 42791;
const providerPort = 42792;
const daemonToken = "playwright-daemon-token";
const windowsExe = path.join(repoRoot, "target", "debug", "autism.exe");
const unixExe = path.join(repoRoot, "target", "debug", "autism");
const rebuildExtensions = new Set([".rs", ".html", ".css", ".js", ".cjs", ".toml", ".lock"]);

let mockServer = null;
let daemonChild = null;
let shuttingDown = false;

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function makeEnv() {
  const appData = path.join(profileDir, "AppData", "Roaming");
  const localAppData = path.join(profileDir, "AppData", "Local");
  ensureDir(appData);
  ensureDir(localAppData);
  ensureDir(inboxDir);
  return {
    ...process.env,
    HOME: profileDir,
    USERPROFILE: profileDir,
    APPDATA: appData,
    LOCALAPPDATA: localAppData,
  };
}

function executablePath() {
  return process.platform === "win32" ? windowsExe : unixExe;
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

function ensureBinaryBuilt() {
  const exe = executablePath();
  if (binaryNeedsRebuild(exe)) {
    const build = spawnSync("cargo", ["build", "-q", "-p", "autism"], {
      cwd: repoRoot,
      stdio: "inherit",
    });
    if (build.status !== 0) {
      throw new Error("failed to build autism binary for dashboard e2e");
    }
  }
  if (!fs.existsSync(exe)) {
    throw new Error(`expected built binary at ${exe}`);
  }
  return exe;
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
  process.exit(code);
}

async function main() {
  fs.rmSync(targetDir, { recursive: true, force: true });
  ensureDir(targetDir);
  const env = makeEnv();
  const exe = ensureBinaryBuilt();
  const configPath = discoverConfigPath(exe, env);
  writeE2EConfig(configPath);
  await startMockProviderServer();
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
