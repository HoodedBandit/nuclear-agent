const fs = require("fs");
const path = require("path");
const { test, expect } = require("@playwright/test");

const statePath = path.resolve(
  __dirname,
  "..",
  "..",
  "target",
  "playwright-e2e",
  "state.json"
);

function readState() {
  return JSON.parse(fs.readFileSync(statePath, "utf8"));
}

async function connectDashboard(page) {
  const state = readState();
  await page.goto("/ui");
  await page.fill("#token-input", state.token);
  await page.click("#connect-button");
  await expect(page.getByTestId("modern-main-workspace")).toBeVisible();
  await expect(page.getByTestId("modern-nav-rail")).toContainText("Nuclear Agent");
  return state;
}

async function openSection(page, name) {
  await page.getByTestId(`nav-${name}`).click();
}

async function openIntegrationsTab(page, tab) {
  await page.locator(`[data-integrations-tab-trigger='${tab}']`).click();
}

test.describe.configure({ mode: "serial" });

test("preserves the legacy static dashboard on the classic route", async ({ page }) => {
  await page.goto("/ui-classic");
  await expect(page).toHaveTitle(/Nuclear Agent Control Room/i);
  await expect(page.locator("body")).toContainText("Resident operator console");
});

test("connects through the dashboard auth form and renders workspace inspection", async ({ page }) => {
  await connectDashboard(page);

  await page.click("#workspace-inspect-submit");
  await expect(page.locator("#workspace-overview")).toContainText("Workspace root");
  await expect(page.locator("#workspace-overview")).toContainText("Cargo.toml");
  await expect(page.locator("#doctor-summary")).toContainText("local-codex");
});

test("runs a chat task with daily mode and restores the transcript context", async ({ page }) => {
  await connectDashboard(page);
  await openSection(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.selectOption("#run-task-mode", "daily");
  await page.fill("#run-task-prompt", "Browser chat daily mode test");
  await page.click("#run-task-submit");

  await expect(page.locator("#chat-transcript")).toContainText("Browser chat daily mode test");
  await expect(page.locator("#chat-transcript")).toContainText(
    "Mock reply from mock-codex: Browser chat daily mode test"
  );
  await expect(page.locator("#sessions-body")).toContainText("daily");
  await expect(page.locator("#session-detail")).toContainText("Recent messages");
});

test("stages an attachment in the cockpit chat form", async ({ page }) => {
  const state = await connectDashboard(page);
  await openSection(page, "chat");

  await page.selectOption("#chat-attachment-kind", "image");
  await page.fill("#chat-attachment-path", state.attachmentPath);
  await page.click("#chat-attachment-add");

  await expect(page.locator("#chat-attachments")).toContainText("reference.png");
  await expect(page.locator("#chat-session-meta")).toContainText("1 attachment(s)");
});

test("creates a provider and alias from the integrations workbench", async ({ page }) => {
  await connectDashboard(page);
  await openSection(page, "integrations");
  await openIntegrationsTab(page, "providers");

  await page.selectOption("#provider-preset", "ollama");
  await page.fill("#provider-id", "ollama-e2e");
  await page.fill("#provider-display-name", "Ollama E2E");
  await page.fill("#provider-default-model", "qwen2.5-coder:7b");
  await page.fill("#provider-alias-name", "ollamae2e");
  await page.click("#provider-save");

  await expect(page.locator("#providers-list")).toContainText("Ollama E2E");
  await expect(page.locator("#providers-list")).toContainText("ollama-e2e");
  await expect(page.locator("#aliases-list")).toContainText("ollamae2e");
});

test("creates an inbox connector from the integrations workbench", async ({ page }) => {
  const state = await connectDashboard(page);
  await openSection(page, "integrations");
  await openIntegrationsTab(page, "connectors");

  await page.selectOption("#connector-kind", "inbox");
  await page.fill("#connector-name", "Inbox E2E");
  await page.fill("#connector-path", state.inboxPath);
  await page.click("#connector-save");

  await expect(page.locator("#connector-roster")).toContainText("Inbox E2E");
  await expect(page.locator("#connector-roster")).toContainText(state.inboxPath);
});

test("installs a local plugin and creates a support bundle", async ({ page }) => {
  const state = await connectDashboard(page);
  await openSection(page, "integrations");
  await openIntegrationsTab(page, "plugins");

  await page.fill("#plugin-install-path", state.pluginPath);
  await page.check("#plugin-install-trusted");
  await page.click("#plugin-install-submit");

  await expect(page.locator("#plugins-list")).toContainText("Echo Toolkit");
  await expect(page.locator("#plugins-health")).toContainText("Echo Toolkit");

  await openSection(page, "system");
  await page.getByRole("button", { name: "diagnostics" }).click();
  await page.click("#support-bundle-submit");

  await expect(page.locator("#support-bundle-result")).toContainText("Bundle ready");
});
