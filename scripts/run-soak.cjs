#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const { performance } = require("perf_hooks");

function parseArgs(argv) {
  const options = {
    baseUrl: process.env.AGENT_BASE_URL || "http://127.0.0.1:42690",
    token: process.env.AGENT_TOKEN || "",
    iterations: 30,
    delayMs: 1000,
    workspacePath: process.env.AGENT_WORKSPACE_PATH || "",
    outputRoot: process.env.AGENT_SOAK_OUTPUT_ROOT || "",
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
    } else if (value === "--output-root" && next) {
      options.outputRoot = next;
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
      "  --output-root <path>  Optional output root for soak artifacts",
      "",
      "Environment fallbacks:",
      "  AGENT_BASE_URL, AGENT_TOKEN, AGENT_WORKSPACE_PATH, AGENT_SOAK_OUTPUT_ROOT",
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

function writeArtifacts(options, samples) {
  const repoRoot = path.resolve(__dirname, "..");
  const outputRoot = path.resolve(options.outputRoot || path.join(repoRoot, "target", "soak"));
  fs.mkdirSync(outputRoot, { recursive: true });
  const runDir = path.join(
    outputRoot,
    new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z").replace("T", "-")
  );
  fs.mkdirSync(runDir, { recursive: true });

  const total = samples.reduce((sum, sample) => sum + sample.durationMs, 0);
  const average = Math.round((total / samples.length) * 100) / 100;
  const slowest = samples.reduce((max, sample) => Math.max(max, sample.durationMs), 0);
  const fastest = samples.reduce((min, sample) => Math.min(min, sample.durationMs), Number.MAX_VALUE);
  const maxProviders = samples.reduce((max, sample) => Math.max(max, sample.providers), 0);
  const maxPlugins = samples.reduce((max, sample) => Math.max(max, sample.plugins), 0);
  const maxSessions = samples.reduce((max, sample) => Math.max(max, sample.sessions), 0);
  const maxDirtyFiles = samples.reduce((max, sample) => Math.max(max, sample.dirtyFiles), 0);

  fs.writeFileSync(
    path.join(runDir, "samples.jsonl"),
    samples.map((sample) => JSON.stringify(sample)).join("\n") + "\n",
    "utf8"
  );

  const summary = {
    base_url: options.baseUrl,
    iterations: samples.length,
    average_ms: average,
    fastest_ms: fastest,
    slowest_ms: slowest,
    max_providers: maxProviders,
    max_plugins: maxPlugins,
    max_sessions: maxSessions,
    max_dirty_files: maxDirtyFiles,
    workspace_root: samples.at(-1)?.workspaceRoot || null,
    run_dir: runDir,
    passed: samples.length,
    failed: 0,
  };
  fs.writeFileSync(path.join(runDir, "summary.json"), JSON.stringify(summary, null, 2) + "\n", "utf8");
  fs.writeFileSync(
    path.join(runDir, "summary.md"),
    [
      "# Soak Summary",
      "",
      `- base_url: \`${summary.base_url}\``,
      `- iterations: \`${summary.iterations}\``,
      `- average_ms: \`${summary.average_ms}\``,
      `- fastest_ms: \`${summary.fastest_ms}\``,
      `- slowest_ms: \`${summary.slowest_ms}\``,
      `- max_providers: \`${summary.max_providers}\``,
      `- max_plugins: \`${summary.max_plugins}\``,
      `- max_sessions: \`${summary.max_sessions}\``,
      `- max_dirty_files: \`${summary.max_dirty_files}\``,
      `- workspace_root: \`${summary.workspace_root || ""}\``,
      "",
    ].join("\n"),
    "utf8"
  );

  return summary;
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

  const summary = writeArtifacts(options, samples);
  process.stdout.write(`Soak output written to ${summary.run_dir}\n`);
  process.stdout.write(JSON.stringify(summary, null, 2) + "\n");
}

main().catch((error) => {
  console.error(error.message || error);
  process.exit(1);
});
