const fs = require("fs");
const path = require("path");
const { test, expect } = require("@playwright/test");

const statePath = path.resolve(__dirname, "..", "..", "target", "playwright-e2e", "state.json");

function readState() {
  return JSON.parse(fs.readFileSync(statePath, "utf8"));
}

async function connectModernDashboard(page) {
  const state = readState();
  await page.goto("/ui-modern");
  await page.fill("#modern-token-input", state.token);
  await page.click("[data-testid='modern-connect-button']");
  await expect(page.locator("[data-testid='modern-dashboard-shell']")).toBeVisible();
  return state;
}

test("modern dashboard connects and renders the cockpit shell", async ({ page }) => {
  await connectModernDashboard(page);
  await expect(page.locator("[data-testid='modern-overview-page']")).toBeVisible();
  await expect(page.locator("[data-testid='nav-chat']")).toBeVisible();
});

test("modern dashboard can run a chat task against the mock provider", async ({ page }) => {
  await connectModernDashboard(page);
  await page.click("[data-testid='nav-chat']");
  await page.selectOption("[data-testid='modern-chat-alias']", "main");
  await page.fill("#modern-chat-prompt", "Modern dashboard smoke");
  await page.click("[data-testid='modern-chat-submit']");

  await expect(page.locator("[data-testid='modern-chat-transcript']")).toContainText(
    "Modern dashboard smoke"
  );
  await expect(page.locator("[data-testid='modern-chat-transcript']")).toContainText(
    "Mock reply from mock-codex: Modern dashboard smoke"
  );
});

test("modern integrations workbench lists providers", async ({ page }) => {
  await connectModernDashboard(page);
  await page.click("[data-testid='nav-integrations']");
  await expect(page.locator("[data-testid='modern-integrations-page']")).toContainText(
    "Local Codex"
  );
});
