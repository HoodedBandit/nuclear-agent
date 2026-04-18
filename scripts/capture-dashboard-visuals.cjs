const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { chromium, expect } = require("@playwright/test");
const {
  VIEWPORTS,
  connectDashboard,
  openDisclosure,
  openRouteTab,
  openSection,
} = require("../tests/dashboard-e2e/helpers.cjs");

const repoRoot = path.resolve(__dirname, "..");
const dashboardBaseUrl = "http://127.0.0.1:42791";
const openClawPort = 4174;

function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    if (!current.startsWith("--")) {
      continue;
    }
    const key = current.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith("--")) {
      args[key] = true;
      continue;
    }
    args[key] = next;
    index += 1;
  }
  return args;
}

function ensureDir(dirPath) {
  fs.mkdirSync(dirPath, { recursive: true });
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch (_) {
      // retry
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function spawnProcess(command, args, options = {}) {
  return spawn(command, args, {
    cwd: options.cwd || repoRoot,
    env: options.env || process.env,
    stdio: options.stdio || "pipe",
    shell: options.shell || false,
  });
}

async function stopProcess(child) {
  if (!child || child.killed) {
    return;
  }
  await new Promise((resolve) => {
    child.once("exit", () => resolve());
    child.kill("SIGTERM");
    setTimeout(() => {
      if (!child.killed) {
        child.kill("SIGKILL");
      }
    }, 5000);
  });
}

async function captureNuclear(outputDir) {
  const coordinator = spawnProcess(process.execPath, ["tests/dashboard-e2e/coordinator.cjs"], {
    cwd: repoRoot,
    stdio: "inherit",
  });

  try {
    await waitForHttp(`${dashboardBaseUrl}/ui`, 180_000);
    const browser = await chromium.launch({ headless: true });

    try {
      const desktopContext = await browser.newContext({
        baseURL: dashboardBaseUrl,
        viewport: VIEWPORTS.desktop,
      });
      const desktop = await desktopContext.newPage();
      const state = await connectDashboard(desktop, expect);

      await openSection(desktop, "overview");
      await desktop.click("#workspace-inspect-submit");
      await desktop.screenshot({
        path: path.join(outputDir, "desktop-overview.png"),
        fullPage: true,
      });

      await openSection(desktop, "chat");
      await desktop.selectOption("#run-task-alias", "main");
      await openDisclosure(desktop, "Runtime overrides");
      await desktop.selectOption("#run-task-mode", "daily");
      await desktop.fill("#run-task-prompt", "Visual port transcript");
      await desktop.click("#run-task-submit");
      await desktop.locator("#chat-transcript").waitFor();
      await desktop.screenshot({
        path: path.join(outputDir, "desktop-chat.png"),
        fullPage: true,
      });

      await openSection(desktop, "channels");
      await desktop.selectOption("#connector-kind", "inbox");
      await desktop.fill("#connector-name", "Inbox Visual");
      await desktop.fill("#connector-path", state.inboxPath);
      await desktop.click("#connector-save");
      await desktop.locator("#connector-roster").waitFor();
      await desktop.screenshot({
        path: path.join(outputDir, "desktop-channels.png"),
        fullPage: true,
      });

      await openSection(desktop, "config");
      await desktop.getByRole("button", { name: "updates" }).click();
      await desktop.click("#update-check-button");
      await desktop.locator("#update-status-body").waitFor();
      await desktop.screenshot({
        path: path.join(outputDir, "desktop-config.png"),
        fullPage: true,
      });

      await openSection(desktop, "debug");
      await desktop.getByRole("button", { name: "support bundle" }).click();
      await desktop.click("#support-bundle-submit");
      await desktop.locator("#support-bundle-result").waitFor();
      await desktop.screenshot({
        path: path.join(outputDir, "desktop-debug.png"),
        fullPage: true,
      });
      await desktop.close();
      await desktopContext.close();

      for (const [viewportName, viewport] of Object.entries({
        tablet: VIEWPORTS.tablet,
        mobile: VIEWPORTS.mobile,
      })) {
        const context = await browser.newContext({
          baseURL: dashboardBaseUrl,
          viewport,
        });
        const page = await context.newPage();
        await connectDashboard(page, expect);

        await page.goto("/ui#/overview");
        await page.click("#workspace-inspect-submit");
        await page.screenshot({
          path: path.join(outputDir, `${viewportName}-overview.png`),
          fullPage: true,
        });

        await page.goto("/ui#/chat");
        await page.selectOption("#run-task-alias", "main");
        await openDisclosure(page, "Runtime overrides");
        await page.selectOption("#run-task-mode", "daily");
        await page.fill("#run-task-prompt", `Visual shell ${viewportName}`);
        await page.click("#run-task-submit");
        await page.locator("#chat-transcript").waitFor();
        await page.screenshot({
          path: path.join(outputDir, `${viewportName}-chat.png`),
          fullPage: true,
        });
        await page.close();
        await context.close();
      }
    } finally {
      await browser.close();
    }
  } finally {
    await stopProcess(coordinator);
  }
}

function npmCommand() {
  return process.platform === "win32" ? "npm.cmd" : "npm";
}

async function ensureOpenClawDependencies(openClawRoot) {
  const nodeModulesPath = path.join(openClawRoot, "node_modules");
  if (fs.existsSync(nodeModulesPath)) {
    return;
  }

  await new Promise((resolve, reject) => {
    const install = spawnProcess(npmCommand(), ["install"], {
      cwd: openClawRoot,
      stdio: "inherit",
      shell: process.platform === "win32",
    });
    install.once("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`OpenClaw dependency install failed with exit code ${code}`));
    });
  });
}

async function prepareOpenClawShell(page, tab) {
  await page.waitForSelector("openclaw-app");
  await page.evaluate((activeTab) => {
    const app = document.querySelector("openclaw-app");
    if (!app) {
      throw new Error("openclaw-app not found");
    }
    app.connected = true;
    app.tab = activeTab;
    app.hello = {
      ok: true,
      server: { version: "2026.4.16" },
      snapshot: { sessionDefaults: { mainSessionKey: "main" } }
    };
    app.chatMessages = activeTab === "chat"
      ? [
          { role: "user", content: "Visual reference prompt", timestamp: Date.now() - 2000 },
          { role: "assistant", content: "Reference response", timestamp: Date.now() - 1000 }
        ]
      : [];
    app.chatToolMessages = [];
    app.eventLog = [
      { level: "info", message: "visual reference", ts: Date.now(), source: "ui" }
    ];
    app.requestUpdate();
  }, tab);
  await page.waitForTimeout(300);
}

async function captureOpenClaw(openClawRoot, outputDir) {
  const uiRoot = path.join(openClawRoot, "ui");
  await ensureOpenClawDependencies(uiRoot);
  const server = spawnProcess(
    npmCommand(),
    ["run", "dev", "--", "--host", "127.0.0.1", "--port", String(openClawPort), "--strictPort"],
    {
      cwd: uiRoot,
      stdio: "inherit",
      shell: process.platform === "win32",
    }
  );

  try {
    await waitForHttp(`http://127.0.0.1:${openClawPort}`, 180_000);
    const browser = await chromium.launch({ headless: true });

    try {
      const captures = [
        ["desktop-overview.png", VIEWPORTS.desktop, "overview"],
        ["desktop-chat.png", VIEWPORTS.desktop, "chat"],
        ["desktop-channels.png", VIEWPORTS.desktop, "channels"],
        ["desktop-config.png", VIEWPORTS.desktop, "config"],
        ["desktop-logs.png", VIEWPORTS.desktop, "logs"],
        ["tablet-overview.png", VIEWPORTS.tablet, "overview"],
        ["tablet-chat.png", VIEWPORTS.tablet, "chat"],
        ["mobile-overview.png", VIEWPORTS.mobile, "overview"],
      ];

      for (const [fileName, viewport, tab] of captures) {
        const page = await browser.newPage({ viewport });
        await page.goto(`http://127.0.0.1:${openClawPort}/${tab}`);
        await prepareOpenClawShell(page, tab);
        await page.screenshot({
          path: path.join(outputDir, fileName),
          fullPage: true,
        });
        await page.close();
      }
    } finally {
      await browser.close();
    }
  } finally {
    await stopProcess(server);
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const nuclearOutput = path.resolve(
    repoRoot,
    args["nuclear-output"] || path.join("target", "dashboard-visual", "nuclear-port")
  );
  ensureDir(nuclearOutput);
  await captureNuclear(nuclearOutput);

  const openClawRoot = args["openclaw-root"] || process.env.OPENCLAW_SOURCE_ROOT;
  if (!openClawRoot) {
    return;
  }

  const openClawOutput = path.resolve(
    repoRoot,
    args["openclaw-output"] || path.join("target", "dashboard-visual", "openclaw-reference")
  );
  ensureDir(openClawOutput);
  await captureOpenClaw(path.resolve(openClawRoot), openClawOutput);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
