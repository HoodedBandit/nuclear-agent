const fs = require("fs");
const path = require("path");
const { test, expect } = require("@playwright/test");

const statePath = path.resolve(__dirname, "..", "..", "target", "playwright-e2e", "state.json");

function readState() {
  return JSON.parse(fs.readFileSync(statePath, "utf8"));
}

function boxesOverlap(a, b) {
  return !(
    a.x + a.width <= b.x ||
    b.x + b.width <= a.x ||
    a.y + a.height <= b.y ||
    b.y + b.height <= a.y
  );
}

async function connectModernDashboard(page) {
  const state = readState();
  await page.goto("/ui");
  await page.fill("#modern-token-input", state.token);
  await page.click("[data-testid='modern-connect-button']");
  await expect(page.locator("[data-testid='modern-dashboard-shell']")).toBeVisible();
  return state;
}

async function openNav(page, section) {
  await page.click(`[data-testid='nav-${section}']`);
}

async function openTab(page, id) {
  await page.click(`[data-testid='modern-${id}']`);
}

async function replaceField(locator, value) {
  await locator.click();
  await locator.press("Control+A");
  await locator.press("Backspace");
  await locator.pressSequentially(value);
}

test.describe.configure({ mode: "serial" });

test("modern dashboard loads the product shell and keeps the main layout panes separated", async ({ page }) => {
  await connectModernDashboard(page);

  const navBox = await page.locator("[data-testid='modern-nav-rail']").boundingBox();
  const workspaceBox = await page.locator("[data-testid='modern-main-workspace']").boundingBox();
  const inspectorBox = await page.locator("[data-testid='modern-right-inspector']").boundingBox();

  expect(navBox).not.toBeNull();
  expect(workspaceBox).not.toBeNull();
  expect(inspectorBox).not.toBeNull();
  expect(boxesOverlap(navBox, workspaceBox)).toBe(false);
  expect(boxesOverlap(workspaceBox, inspectorBox)).toBe(false);

  await page.click("[data-testid='modern-overview-tab-workspace']");
  await expect(page.locator("#workspace-summary-cards")).toContainText("Workspace root");
  await expect(page.locator("#workspace")).toContainText("crates");
  await expect(page.getByRole("heading", { name: "Manifests" })).toBeVisible();
  await expect(page.locator("article").filter({ hasText: "Cargo.toml" }).first()).toBeVisible();
  await expect(page.locator("#workspace-overview")).toContainText(".rs");
  await page.click("[data-testid='modern-overview-tab-summary']");
  await expect(page.locator("[data-testid='modern-overview-page']")).toContainText("Local Codex");
});

test("modern navigation swaps product workbenches cleanly without leaving stale panels active", async ({ page }) => {
  await connectModernDashboard(page);

  await openNav(page, "chat");
  await expect(page.locator("#run-task-submit")).toBeVisible();
  await expect(page.locator("#providers-list")).toBeHidden();

  await openNav(page, "integrations");
  await expect(page.locator("#providers-list")).toBeVisible();
  await expect(page.locator("#run-task-submit")).toBeHidden();

  await openTab(page, "integrations-tab-connectors");
  await expect(page.getByRole("heading", { name: "Inbox setup" })).toBeVisible();
  await expect(page.locator("#connector-roster")).toBeVisible();

  await openNav(page, "system");
  await openTab(page, "system-tab-config");
  await expect(page.locator("#advanced-config-load")).toBeVisible();
  await expect(page.locator("#connector-roster")).toBeHidden();
});

test("modern chat promotes the selected alias to main and keeps it after reload", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");

  await page.selectOption("#run-task-alias", "claude");
  await page.click("#chat-make-main-button");
  await expect(page.locator("[data-testid='modern-nav-rail']")).toContainText("claude");

  await page.reload();
  await expect(page.locator("[data-testid='modern-nav-rail']")).toContainText("claude");
  await expect(page.locator("#run-task-alias")).toHaveValue("claude");
});

test("modern chat runs a task, preserves task mode, and exposes the resume packet", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.selectOption("#run-task-mode", "daily");
  await page.fill("#modern-chat-prompt", "Modern cockpit session");
  await page.click("#run-task-submit");

  await expect(page.locator("#chat-transcript")).toContainText("Modern cockpit session");
  await expect(page.locator("#chat-transcript")).toContainText("Mock reply from mock-codex: Modern cockpit session");

  await page.click("[data-testid='modern-chat-tab-sessions']");
  const sessionButton = page.locator("#sessions-body button").first();
  const sessionId = await sessionButton.getAttribute("data-session-id");
  expect(sessionId).toBeTruthy();
  await sessionButton.click();

  await expect(page.locator("#chat-session-meta")).toContainText("Daily");
  await expect(page.locator("#session-detail")).toContainText("Alias");
  await expect(page.getByText("Linked context", { exact: true })).toBeVisible();
});

test("modern chat restores task mode when a saved session is reopened", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.selectOption("#run-task-mode", "daily");
  await page.fill("#modern-chat-prompt", "Daily mode persistence");
  await page.click("#run-task-submit");

  await expect(page.locator("#chat-transcript")).toContainText("Daily mode persistence");
  await page.click("[data-testid='modern-chat-tab-sessions']");
  const sessionButton = page.locator("#sessions-body button").first();
  await sessionButton.click();
  await expect(page.locator("#chat-session-meta")).toContainText("Daily");

  await page.click("[data-testid='modern-chat-tab-run']");
  await page.click("#chat-new-session");
  await page.selectOption("#run-task-mode", "");
  await page.click("[data-testid='modern-chat-tab-sessions']");
  await sessionButton.click();

  await page.click("[data-testid='modern-chat-tab-run']");
  await expect(page.locator("#run-task-mode")).toHaveValue("daily");
  await page.click("[data-testid='modern-chat-tab-sessions']");
  await expect(page.locator("#chat-session-meta")).toContainText("Daily");
});

test("modern chat keeps control-socket semantics and does not fall back to HTTP streaming", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");

  let fallbackCalls = 0;
  await page.route("**/v1/run/stream", async (route) => {
    fallbackCalls += 1;
    await route.fulfill({ status: 200, contentType: "text/plain", body: "" });
  });

  await page.fill("#modern-chat-prompt", "Guarded fallback smoke");
  await page.click("#run-task-submit");

  await expect
    .poll(() =>
      page.evaluate(() => window.nuclearDashboardDebug.getPendingControlRequestCount())
    )
    .toBeGreaterThan(0);

  await page.evaluate(() => window.nuclearDashboardDebug.dropControlSocket());

  await expect(page.locator("#run-task-error")).toContainText("Control socket disconnected");
  await expect.poll(() => fallbackCalls).toBe(0);
});

test("modern providers workbench creates a provider and alias", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "integrations");
  await page.click("[data-testid='modern-integrations-tab-providers']");

  await page.selectOption("label:has-text('Provider preset') select", "ollama");
  await page.fill("label:has-text('Provider ID') input", "ollama-e2e");
  await page.fill("label:has-text('Display name') input", "Ollama E2E");
  await page.fill("label:has-text('Default model') input", "qwen2.5-coder:7b");
  await page.fill("label:has-text('Alias name') input", "ollamae2e");
  await page.click("[data-testid='modern-provider-save']");

  await expect(page.locator("#providers-list")).toContainText("Ollama E2E");
  await expect(page.locator("#aliases-list")).toContainText("ollamae2e");
});

test("modern overview quick launch opens the correct guided workbenches", async ({ page }) => {
  await connectModernDashboard(page);
  await page.click("[data-testid='modern-overview-tab-summary']");

  await page.locator("#setup-checklist").getByRole("button", { name: "Telegram" }).click();
  await expect(page).toHaveURL(/integrations\?tab=connectors&connector=telegram/);
  await expect(page.getByRole("heading", { name: "Telegram setup" })).toBeVisible();
  await expect(page.locator("label:has-text('Bot token')")).toBeVisible();

  await openNav(page, "overview");
  await page.locator("#setup-checklist").getByRole("button", { name: "Permissions" }).click();
  await expect(page).toHaveURL(/system\?tab=trust&focus=permissions/);
  await expect(page.locator("#permissions")).toBeVisible();
});

test("modern connectors workbench creates an inbox connector", async ({ page }) => {
  const state = await connectModernDashboard(page);
  await openNav(page, "integrations");
  await page.click("[data-testid='modern-integrations-tab-connectors']");
  await expect(page).toHaveURL(/integrations\?tab=connectors/);
  await expect(page.getByRole("heading", { name: "Inbox setup" })).toBeVisible();

  await replaceField(page.locator("#connector-id"), "inbox-e2e");
  await replaceField(page.locator("#connector-name"), "Inbox E2E");
  await replaceField(page.locator("#connector-inbox-path"), state.inboxPath);
  await page.click("#connector-save");

  await expect(page.locator("#connector-roster")).toContainText("Inbox E2E");
  await expect(page.locator("#connector-roster")).toContainText(state.inboxPath);
});

test("modern plugins workbench installs, updates, and removes a plugin", async ({ page }) => {
  const state = await connectModernDashboard(page);
  await openNav(page, "integrations");
  await page.click("[data-testid='modern-integrations-tab-plugins']");

  await page.fill("label:has-text('Source path') input", state.pluginPath);
  await page.getByLabel("Trust immediately").check();
  await page.click("button:has-text('Install plugin')");

  await expect(page.locator("#plugins-list")).toContainText("Echo Toolkit");
  await expect(page.locator("#plugins-health")).toContainText("ready");

  const pluginCard = page.locator("#plugins-list article").filter({ hasText: "echo-toolkit" });
  fs.appendFileSync(path.join(state.pluginPath, "tool.js"), `\n// update ${Date.now()}\n`);
  await pluginCard.getByRole("button", { name: "Update" }).click();
  await pluginCard.getByRole("button", { name: "Disable" }).click();
  await expect(pluginCard).toContainText("Disabled");
  await pluginCard.getByRole("button", { name: "Enable" }).click();
  await expect(pluginCard).toContainText("Enabled");
  await pluginCard.getByRole("button", { name: "Remove" }).click();
  await expect(page.getByText("No plugins installed yet.")).toBeVisible();
});

test("modern operations workbench rebuilds and searches memory", async ({ page }) => {
  const prompt = "I prefer concise output.";
  await connectModernDashboard(page);
  await openNav(page, "chat");
  await page.fill("#modern-chat-prompt", prompt);
  await page.click("#run-task-submit");
  await expect(page.locator("#chat-transcript")).toContainText(prompt);

  await page.click("[data-testid='modern-chat-tab-sessions']");
  const sessionRow = page.locator("#sessions-body tr").filter({ hasText: prompt }).first();
  const sessionId = await sessionRow.getByRole("button", { name: "View" }).getAttribute("data-session-view");

  await openNav(page, "operations");
  await page.click("[data-testid='modern-operations-tab-memory']");
  await page.fill("#memory-rebuild-session-id", sessionId || "");
  await page.click("#memory-rebuild-form button[type='submit']");
  await expect(page.locator("text=Upserted")).toBeVisible();

  const candidateReviewCard = page.locator("#memory-review-queue article").first();
  await expect(candidateReviewCard).toBeVisible();
  await candidateReviewCard.getByRole("button", { name: "Approve" }).click();

  const memorySubject = "Modern dashboard memory";
  await page.fill("#memory-create-subject", memorySubject);
  await page.fill("#memory-create-content", "This memory should be searchable from the modern cockpit.");
  await page.click("#memory-create-form button[type='submit']");
  await expect(page.locator("#memory-create-form")).toContainText(`Saved memory "${memorySubject}" as Accepted.`);

  await page.fill("#memory-search-query", memorySubject);
  await page.click("#memory-search-form button[type='submit']");
  await expect(page.locator("#memory-search-results")).toContainText(memorySubject);

  await openNav(page, "chat");
  await page.click("[data-testid='modern-chat-tab-sessions']");
  await sessionRow.getByRole("button", { name: "View" }).click();
  await expect(page.locator("#session-detail")).toContainText("Alias");
  await expect(page.getByRole("heading", { name: "Recent messages" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Linked memories" })).toBeVisible();
  await expect(page.getByText("evidence:", { exact: false }).first()).toBeVisible();
  await page.click("[data-testid='modern-chat-tab-context']");
  await expect(page.getByRole("heading", { name: "Related transcript hits" })).toBeVisible();
});

test("modern system workbench loads and saves config and preserves self-edit when enabling autonomy", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "system");

  await page.click("[data-testid='modern-system-tab-trust']");
  const selfEdit = page.locator('[data-trust-flag="allow_self_edit"]');
  if (await selfEdit.isChecked()) {
    await selfEdit.uncheck();
  }
  await page.getByRole("button", { name: "Free thinking" }).click();
  await expect(page.locator("#autonomy-mode")).toHaveText("Free Thinking");
  await expect(selfEdit).not.toBeChecked();

  await page.click("[data-testid='modern-system-tab-config']");
  await page.click("#advanced-config-load");
  await expect(page.locator("#advanced-config-editor")).toHaveValue(/"providers"/);

  const raw = await page.locator("#advanced-config-editor").inputValue();
  const parsed = JSON.parse(raw);
  parsed.thinking_level = "medium";
  await page.fill("#advanced-config-editor", JSON.stringify(parsed));
  await page.click("#advanced-config-format");
  await page.click("#advanced-config-save");
  await expect(page.locator("#advanced-config-summary")).toContainText("Saved full config");
});

test("modern chat surfaces remote-content safety events", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");
  await expect
    .poll(() =>
      page.evaluate(() => typeof window.nuclearDashboardDebug?.emitChatStreamEvent)
    )
    .toBe("function");

  await page.evaluate(() => {
    window.nuclearDashboardDebug.emitChatStreamEvent({
      type: "remote_content",
      artifact: {
        source: {
          kind: "web_page",
          label: "malicious page",
          url: "https://example.com",
          host: "example.com"
        },
        title: "malicious",
        mime_type: "text/html",
        excerpt: "Blocked suspicious remote content from example.com.",
        assessment: {
          risk: "high",
          blocked: true,
          reasons: ["remote content requests secrets, credentials, or hidden prompts"],
          warnings: ["instruction override language detected"]
        }
      }
    });
  });

  await expect(page.locator("#chat-remote-content")).toContainText("malicious page");
  await expect(page.locator("#chat-remote-content")).toContainText("blocked");
});

test("modern chat runs slash commands and shell commands from the cockpit", async ({ page }) => {
  await connectModernDashboard(page);
  await openNav(page, "chat");

  await page.fill("#modern-chat-prompt", "/status");
  await page.click("#run-task-submit");
  await expect(page.locator("#run-task-result")).toContainText("permission_preset=");

  await page.fill("#chat-attachment-path", "J:\\\\images\\\\console.png");
  await page.click("#chat-attachment-add");
  await expect(page.locator("#chat-attachments")).toContainText("console.png");

  await page.fill("#modern-chat-prompt", "/attachments");
  await page.click("#run-task-submit");
  await expect(page.locator("#run-task-result")).toContainText("console.png");

  await page.fill("#modern-chat-prompt", "!cd .");
  await page.click("#run-task-submit");
  await expect(page.locator("#run-task-result")).toContainText("cwd=");
});
