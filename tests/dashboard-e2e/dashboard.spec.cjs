const fs = require("fs");
const path = require("path");
const { test, expect } = require("@playwright/test");

const statePath = path.resolve(__dirname, "..", "..", "target", "playwright-e2e", "state.json");

function readState() {
  return JSON.parse(fs.readFileSync(statePath, "utf8"));
}

async function connectDashboard(page) {
  const state = readState();
  await page.goto("/ui");
  await page.fill("#token-input", state.token);
  await page.click("#connect-button");
  await expect(page.locator("#connection-status")).toContainText("Connected.");
  await expect(page.locator("#providers-summary")).toContainText("provider");
  await expect(page.locator("#providers-list")).toContainText("Local Codex");
  return state;
}

test.describe.configure({ mode: "serial" });

test("connects through the dashboard auth form and loads provider state", async ({ page }) => {
  await connectDashboard(page);
  await expect(page.locator("#aliases-list")).toContainText("main");
  await expect(page.locator("#chat-main-target")).toContainText("main");
  await expect(page.locator("#run-task-alias")).toHaveValue("main");
});

test("switches the active alias and promotes it to main", async ({ page }) => {
  await connectDashboard(page);

  await page.selectOption("#run-task-alias", "claude");
  await page.click("#chat-make-main-button");

  await expect(page.locator("#chat-main-target")).toContainText("claude");
  await page.reload();
  await expect(page.locator("#chat-main-target")).toContainText("claude");
});

test("sends a chat message through the dashboard against the mock local provider", async ({ page }) => {
  await connectDashboard(page);

  await page.selectOption("#run-task-alias", "main");
  await page.fill("#run-task-prompt", "Browser chat test");
  await page.click("#run-task-form button[type='submit']");

  await expect(page.locator("#chat-transcript")).toContainText("Browser chat test");
  await expect(page.locator("#chat-transcript")).toContainText("Mock reply from mock-codex: Browser chat test");
  await expect(page.locator("#sessions-body")).toContainText("main");
});

test("creates an inbox connector from the dashboard", async ({ page }) => {
  const state = await connectDashboard(page);

  const connectorId = `inbox-e2e-${Date.now()}`;
  await page.selectOption("#connector-add-type", "inbox");
  await page.fill("#connector-add-name", "Inbox E2E");
  await page.fill("#connector-add-id", connectorId);
  await page.fill("#connector-add-description", "Inbox connector from Playwright");
  await page.fill("#connector-add-alias", "main");
  await page.locator("[data-connector-field='path']").fill(state.inboxPath);
  await page.check("#connector-add-enabled");
  await page.click("#connector-submit");

  await expect(page.locator("#connectors-body")).toContainText("Inbox E2E");
  await expect(page.locator("#connectors-body")).toContainText(connectorId);
});
