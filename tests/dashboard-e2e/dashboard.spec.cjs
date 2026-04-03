const fs = require("fs");
const path = require("path");
const { test, expect } = require("@playwright/test");

const statePath = path.resolve(__dirname, "..", "..", "target", "playwright-e2e", "state.json");

function readState() {
  return JSON.parse(fs.readFileSync(statePath, "utf8"));
}

async function connectDashboard(page) {
  await page.addInitScript(() => {
    window.__dashboardTestMode = true;
  });
  const state = readState();
  await page.goto("/ui");
  await page.fill("#token-input", state.token);
  await page.click("#connect-button");
  await expect
    .poll(async () => (await page.locator("#connection-status").textContent()) || "", {
      timeout: 10000,
    })
    .toMatch(/Connected\.|Live control connection lost\./);
  await expect(page.locator("#providers-summary")).toContainText("provider");
  await expect(page.locator("#providers-list")).toContainText("Local Codex");
  return state;
}

async function openTab(page, tabId) {
  await page.click(`[data-dashboard-tab-trigger='${tabId}']`);
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
  await openTab(page, "chat");

  await page.selectOption("#run-task-alias", "claude");
  await page.click("#chat-make-main-button");

  await expect(page.locator("#chat-main-target")).toContainText("claude");
  await page.reload();
  await expect(page.locator("#chat-main-target")).toContainText("claude");
});

test("sends a chat message through the dashboard against the mock local provider", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.fill("#run-task-prompt", "Browser chat test");
  await page.click("#run-task-form button[type='submit']");

  await expect(page.locator("#chat-transcript")).toContainText("Browser chat test");
  await expect(page.locator("#chat-transcript")).toContainText("Mock reply from mock-codex: Browser chat test");
  await expect(page.locator("#sessions-body")).toContainText("main");
});

test("does not fall back to HTTP when a control-socket request disconnects after dispatch", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  let fallbackCalls = 0;
  await page.route("**/v1/run/stream", async (route) => {
    fallbackCalls += 1;
    await route.fulfill({
      status: 200,
      contentType: "text/plain",
      body: "",
    });
  });

  await page.fill("#run-task-prompt", "Guarded fallback smoke");
  await page.click("#run-task-form button[type='submit']");

  await expect
    .poll(async () =>
      page.evaluate(() => window.dashboardApp.__debug.getPendingControlRequestCount())
    )
    .toBeGreaterThan(0);

  await page.evaluate(() => window.dashboardApp.__debug.dropControlSocket());

  await expect(page.locator("#connection-status")).toContainText(
    "Live control connection was lost after the task was already dispatched"
  );
  await expect(page.locator("#run-task-result")).toContainText("Chat failed");
  await expect.poll(() => fallbackCalls).toBe(0);
});

test("restores the saved task mode when reloading a chat session", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.selectOption("#run-task-mode", "daily");
  await page.fill("#run-task-prompt", "Daily mode persistence");
  await page.click("#run-task-form button[type='submit']");

  await expect(page.locator("#chat-transcript")).toContainText("Mock reply from mock-codex: Daily mode persistence");
  await expect(page.locator("#sessions-body")).toContainText("daily");
  await page.click("#chat-new-session");
  await page.selectOption("#run-task-mode", "");
  await page.locator("#sessions-body tr").first().getByRole("button", { name: "Chat" }).click();

  await expect(page.locator("#run-task-mode")).toHaveValue("daily");
  await expect(page.locator("#chat-session-meta")).toContainText("daily");
});

test("shows the session resume packet from the dashboard", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.fill("#run-task-prompt", "Resume packet dashboard test");
  await page.click("#run-task-form button[type='submit']");
  await expect(page.locator("#chat-transcript")).toContainText("Resume packet dashboard test");

  const sessionRow = page.locator("#sessions-body tr").filter({ hasText: "Resume packet dashboard test" }).first();
  await sessionRow.getByRole("button", { name: "View" }).click();
  await expect(page.locator("#session-detail")).toContainText("Recent messages");
  await expect(page.locator("#session-detail")).toContainText("Related transcript hits");
});

test("creates a provider from the guided provider workbench", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "integrations");

  await page.locator("[data-provider-select='ollama']").click();
  await page.locator("#provider-name").fill("Ollama E2E");
  await page.locator("#provider-default-model").fill("qwen2.5-coder:7b");
  await page.locator("#provider-alias-name").fill("ollamae2e");
  await page.click("#provider-save");

  await expect(page.locator("#providers-list")).toContainText("Ollama E2E");
  await expect(page.locator("#providers-list")).toContainText("ollama-local");
  await expect(page.locator("#aliases-list")).toContainText("ollamae2e");
});

test("operator launchpad shortcuts open guided setup tools", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "overview");

  await page.locator("#setup-checklist").getByRole("button", { name: "Telegram" }).click();
  await expect(page.locator("#connector-workbench-title")).toContainText("Telegram");
  await expect(page.locator("#connector-quick-form")).toContainText("Bot token");

  await openTab(page, "overview");
  await page.locator("#setup-checklist").getByRole("button", { name: "Permissions" }).click();
  await expect
    .poll(() =>
      page.locator("#permissions").evaluate((element) => Math.round(element.getBoundingClientRect().top))
    )
    .toBeLessThan(180);
});

test("creates an inbox connector from the dashboard", async ({ page }) => {
  const state = await connectDashboard(page);
  await openTab(page, "integrations");

  await page.locator("[data-connector-select='inbox']").click();
  await page.locator("#connector-quick-form [data-connector-field='name']").fill("Inbox E2E");
  await page.locator("#connector-quick-form [data-connector-field='path']").fill(state.inboxPath);
  await page.click("#connector-save");

  await expect(page.locator("#connector-roster")).toContainText("Inbox E2E");
  await expect(page.locator("#connector-roster")).toContainText(state.inboxPath);
});

test("inspects the current workspace from the dashboard", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "overview");

  const layoutWidth = await page.locator(".layout").evaluate((element) =>
    Math.round(element.getBoundingClientRect().width)
  );
  await expect
    .poll(() =>
      page.locator("#workspace").evaluate((element) =>
        Math.round(element.getBoundingClientRect().width)
      )
    )
    .toBeGreaterThan(Math.round(layoutWidth * 0.7));

  await expect(page.locator("#workspace-summary-cards")).toContainText("Workspace root");
  await expect(page.locator("#workspace-overview")).toContainText("Cargo.toml");
  await expect(page.locator("#workspace-overview")).toContainText("README.md");
});

test("manages a local plugin from the dashboard", async ({ page }) => {
  const state = await connectDashboard(page);
  await openTab(page, "integrations");
  const layoutWidth = await page.locator(".layout").evaluate((element) =>
    Math.round(element.getBoundingClientRect().width)
  );
  await expect
    .poll(() =>
      page.locator("#plugins").evaluate((element) =>
        Math.round(element.getBoundingClientRect().width)
      )
    )
    .toBeGreaterThan(Math.round(layoutWidth * 0.7));

  await page.fill("#plugin-install-path", state.pluginPath);
  await page.evaluate(() => {
    const input = document.getElementById("plugin-install-trusted");
    if (input) {
      input.checked = true;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }
  });
  await page.locator("#plugin-install-form").evaluate((form) => form.requestSubmit());

  const pluginCard = page
    .locator("#plugins-list .stack-card")
    .filter({ hasText: "echo-toolkit" });
  await expect(pluginCard).toContainText("Echo Toolkit");
  await expect(pluginCard).toContainText("trusted");
  await expect(pluginCard).toContainText("review current");

  const integrityBefore = await pluginCard.locator(".micro-list").textContent();
  fs.appendFileSync(path.join(state.pluginPath, "tool.js"), `\n// update ${Date.now()}\n`);

  await pluginCard.getByRole("button", { name: "Update" }).click();
  await expect
    .poll(async () => pluginCard.locator(".micro-list").textContent())
    .not.toBe(integrityBefore);
  await expect(pluginCard).toContainText("review needed");
  await pluginCard.getByRole("button", { name: "Trust" }).click();

  await pluginCard.getByRole("button", { name: "Doctor" }).click();
  await expect(page.locator("#plugins-health")).toContainText("Echo Toolkit");
  await expect(page.locator("#plugins-health")).toContainText("ready");

  await pluginCard.getByRole("button", { name: "Disable" }).click();
  await expect(pluginCard).toContainText("disabled");

  await pluginCard.getByRole("button", { name: "Enable" }).click();
  await expect(pluginCard).toContainText("enabled");

  page.once("dialog", (dialog) => dialog.accept());
  await pluginCard.getByRole("button", { name: "Remove" }).click();
  await expect(page.locator("#plugins-list")).not.toContainText("echo-toolkit");
});

test("loads and saves the advanced dashboard config editor", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "system");

  await page.click("#advanced-config-load");
  await expect(page.locator("#advanced-config-editor")).toHaveValue(/"providers"/);

  const raw = await page.locator("#advanced-config-editor").inputValue();
  const parsed = JSON.parse(raw);
  parsed.thinking_level = "medium";

  await page.locator("#advanced-config-editor").fill(JSON.stringify(parsed));
  await page.click("#advanced-config-format");
  await page.click("#advanced-config-save");

  await expect(page.locator("#advanced-config-summary")).toContainText("Saved full config");
});

test("switches tabs and only shows the relevant panel groups", async ({ page }) => {
  await connectDashboard(page);

  await openTab(page, "chat");
  await expect(page.locator("#run-task")).toBeVisible();
  await expect(page.locator("#sessions")).toBeVisible();
  await expect(page.locator("#providers")).toBeHidden();

  await openTab(page, "integrations");
  await expect(page.locator("#providers")).toBeVisible();
  await expect(page.locator("#connectors")).toBeVisible();
  await expect(page.locator("#run-task")).toBeHidden();
});

test("rebuilds memory from session ledger and surfaces it in search and resume views", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.selectOption("#run-task-alias", "main");
  await page.fill("#run-task-prompt", "I prefer concise output.");
  await page.click("#run-task-form button[type='submit']");

  await expect(page.locator("#chat-transcript")).toContainText("I prefer concise output.");

  const sessionId = await page
    .locator("#sessions-body tr")
    .first()
    .getByRole("button", { name: "View" })
    .evaluate((button) => button.dataset.sessionView);

  await openTab(page, "operations");
  await page.fill("#memory-rebuild-session-id", sessionId || "");
  await page.click("#memory-rebuild-form button[type='submit']");

  const memorySubject = "Dashboard memory smoke";
  await page.locator("#memory-create-subject").fill(memorySubject);
  await page
    .locator("#memory-create-content")
    .fill("This dashboard memory should expose provenance and evidence.");
  await page.locator("#memory-create-form").evaluate((form) => form.requestSubmit());

  await page.fill("#memory-search-query", memorySubject);
  await page.click("#memory-search-form button[type='submit']");
  await expect(page.locator("#memory-search-results")).toContainText(memorySubject);
  await expect(page.locator("#memory-search-results")).toContainText("evidence:");

  await openTab(page, "chat");
  await page.locator("#sessions-body tr").first().getByRole("button", { name: "View" }).click();
  await expect(page.locator("#session-detail")).toContainText("Linked memories");
  await expect(page.locator("#session-detail")).toContainText("Recent messages");
  await expect(page.locator("#session-detail")).not.toContainText("No linked memories.");
  await expect(page.locator("#session-detail")).toContainText("evidence:");
});

test("runs slash commands and shell commands from the dashboard chat console", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.fill("#run-task-prompt", "/status");
  await page.click("#run-task-form button[type='submit']");
  await expect(page.locator("#run-task-result")).toContainText("permission_preset=");

  await page.fill("#chat-attachment-path", "J:\\\\images\\\\console.png");
  await page.click("#chat-attachment-add");
  await expect(page.locator("#chat-attachments")).toContainText("console.png");

  await page.fill("#run-task-prompt", "/attachments");
  await page.click("#run-task-form button[type='submit']");
  await expect(page.locator("#run-task-result")).toContainText("console.png");

  await page.fill("#run-task-prompt", "!cd .");
  await page.click("#run-task-form button[type='submit']");
  await expect(page.locator("#run-task-result")).toContainText("cwd=");
});

test("keeps self-edit unchanged when enabling autonomy", async ({ page }) => {
  await connectDashboard(page);

  const selfEdit = page.locator('[data-trust-flag="allow_self_edit"]');
  if (await selfEdit.isChecked()) {
    await selfEdit.uncheck();
    await expect(selfEdit).not.toBeChecked();
  }

  await page.getByRole("button", { name: "Free thinking" }).click();
  await expect(page.locator("#control-summary")).toContainText("free_thinking");
  await expect(selfEdit).not.toBeChecked();
});

test("surfaces remote-content safety events in the operator console", async ({ page }) => {
  await connectDashboard(page);
  await openTab(page, "chat");

  await page.evaluate(() => {
    window.dashboardApp.__debug.emitChatStreamEvent({
      type: "remote_content",
      artifact: {
        source: {
          kind: "web_page",
          label: "malicious page",
          url: "https://example.com",
          host: "example.com",
        },
        title: "malicious",
        mime_type: "text/html",
        excerpt: "Blocked suspicious remote content from example.com.",
        assessment: {
          risk: "high",
          blocked: true,
          reasons: ["remote content requests secrets, credentials, or hidden prompts"],
          warnings: ["instruction override language detected"],
        },
      },
    });
  });

  await expect(page.locator("#run-task-result")).toContainText("Remote content");
  await expect(page.locator("#run-task-result")).toContainText("Blocked remote content from malicious page");
  await expect(page.locator("#chat-transcript")).toContainText("remote_content");
});
