#!/usr/bin/env node

const { performance } = require("perf_hooks");

function parseArgs(argv) {
  const options = {
    baseUrl: process.env.AGENT_BASE_URL || "http://127.0.0.1:42690",
    token: process.env.AGENT_TOKEN || "",
    iterations: 30,
    delayMs: 1000,
    workspacePath: process.env.AGENT_WORKSPACE_PATH || "",
  };

  for (let index = 2; index < argv.length; index += 1) {
    const value = argv[index];
    const next = argv[index + 1];
    if (value === "--base-url" && next) {
      options.baseUrl = next;
      index += 1;
    } else if (value === "--token" && next) {
      options.token = next;
      index += 1;
    } else if (value === "--iterations" && next) {
      options.iterations = Math.max(1, Number.parseInt(next, 10) || 1);
      index += 1;
    } else if (value === "--delay-ms" && next) {
      options.delayMs = Math.max(0, Number.parseInt(next, 10) || 0);
      index += 1;
    } else if (value === "--workspace" && next) {
      options.workspacePath = next;
      index += 1;
    } else if (value === "--help" || value === "-h") {
      printHelp();
      process.exit(0);
    }
  }

  if (!options.token) {
    throw new Error("A daemon token is required. Pass --token or set AGENT_TOKEN.");
  }
  return options;
}

function printHelp() {
  process.stdout.write(
    [
      "Usage: node scripts/run-soak.cjs --token <daemon-token> [options]",
      "",
      "Options:",
      "  --base-url <url>      Daemon base URL (default http://127.0.0.1:42690)",
      "  --iterations <n>      Number of loop iterations (default 30)",
      "  --delay-ms <n>        Delay between iterations in milliseconds (default 1000)",
      "  --workspace <path>    Optional workspace path for /v1/workspace/inspect",
      "",
      "Environment fallbacks:",
      "  AGENT_BASE_URL, AGENT_TOKEN, AGENT_WORKSPACE_PATH",
      "",
    ].join("\n")
  );
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function requestJson(baseUrl, token, path, options = {}) {
  const response = await fetch(new URL(path, baseUrl), {
    ...options,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      ...(options.headers || {}),
    },
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}${text ? `: ${text}` : ""}`);
  }
  return response.json();
}

async function runIteration(options, iteration) {
  const startedAt = performance.now();
  const status = await requestJson(options.baseUrl, options.token, "/v1/status");
  const bootstrap = await requestJson(options.baseUrl, options.token, "/v1/dashboard/bootstrap");
  const workspace = await requestJson(options.baseUrl, options.token, "/v1/workspace/inspect", {
    method: "POST",
    body: JSON.stringify({
      path: options.workspacePath || null,
    }),
  });
  const durationMs = Math.round((performance.now() - startedAt) * 100) / 100;
  return {
    iteration,
    durationMs,
    providers: status.providers,
    plugins: status.plugins,
    sessions: Array.isArray(bootstrap.sessions) ? bootstrap.sessions.length : 0,
    workspaceRoot: workspace.workspace_root,
    dirtyFiles: workspace.dirty_files,
  };
}

async function main() {
  const options = parseArgs(process.argv);
  const samples = [];

  for (let iteration = 1; iteration <= options.iterations; iteration += 1) {
    const sample = await runIteration(options, iteration);
    samples.push(sample);
    process.stdout.write(
      `[soak] ${String(iteration).padStart(3, "0")}/${options.iterations} ${sample.durationMs}ms providers=${sample.providers} plugins=${sample.plugins} sessions=${sample.sessions} dirty=${sample.dirtyFiles}\n`
    );
    if (iteration < options.iterations && options.delayMs > 0) {
      await sleep(options.delayMs);
    }
  }

  const total = samples.reduce((sum, sample) => sum + sample.durationMs, 0);
  const average = Math.round((total / samples.length) * 100) / 100;
  const slowest = samples.reduce((max, sample) => Math.max(max, sample.durationMs), 0);
  const fastest = samples.reduce((min, sample) => Math.min(min, sample.durationMs), Number.MAX_VALUE);
  process.stdout.write(
    JSON.stringify(
      {
        iterations: samples.length,
        average_ms: average,
        fastest_ms: fastest,
        slowest_ms: slowest,
        workspace_root: samples.at(-1)?.workspaceRoot || null,
      },
      null,
      2
    ) + "\n"
  );
}

main().catch((error) => {
  console.error(error.message || error);
  process.exit(1);
});
