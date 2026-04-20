const { test, expect } = require("@playwright/test");
const {
  VIEWPORTS,
  assertShellLayout,
  connectDashboard,
  openDisclosure,
  openRouteTab,
  openSection,
} = require("./helpers.cjs");

async function captureArtifact(page, testInfo, name) {
  const filePath = testInfo.outputPath(name);
  await page.screenshot({ path: filePath, fullPage: true });
  await testInfo.attach(name, {
    path: filePath,
    contentType: "image/png",
  });
}

test.describe.configure({ mode: "serial" });

for (const [viewportName, viewport] of Object.entries(VIEWPORTS)) {
  test(`overview shell stays bounded on ${viewportName}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await connectDashboard(page, expect);
    await openSection(page, "overview");
    await page.click("#workspace-inspect-submit");
    await expect(page.locator("#workspace-overview")).toContainText("Workspace root");
    await assertShellLayout(page, expect);
    await captureArtifact(page, testInfo, `${viewportName}-overview.png`);
  });

  test(`chat shell stays bounded on ${viewportName}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await connectDashboard(page, expect);
    await openSection(page, "chat");
    await page.selectOption("#run-task-alias", "main");
    await openDisclosure(page, "Runtime overrides");
    await page.selectOption("#run-task-mode", "daily");
    await page.fill("#run-task-prompt", `Visual shell ${viewportName}`);
    await page.click("#run-task-submit");
    await expect(page.locator("#chat-transcript")).toContainText(`Visual shell ${viewportName}`);
    await assertShellLayout(page, expect);
    await captureArtifact(page, testInfo, `${viewportName}-chat.png`);
  });
}

test("operator surfaces stay bounded on desktop", async ({ page }, testInfo) => {
  await page.setViewportSize(VIEWPORTS.desktop);
  const state = await connectDashboard(page, expect);

  await openSection(page, "channels");
  await page.selectOption("#connector-kind", "inbox");
  await page.fill("#connector-name", "Inbox Visual");
  await page.fill("#connector-path", state.inboxPath);
  await page.click("#connector-save");
  await expect(page.locator("#connector-roster")).toContainText("Inbox Visual");
  await assertShellLayout(page, expect);
  await captureArtifact(page, testInfo, "desktop-channels.png");

  await openSection(page, "config");
  await page.getByRole("button", { name: "updates" }).click();
  await page.click("#update-check-button");
  await expect(page.locator("#update-status-body")).toContainText("0.8.4 is available");
  await assertShellLayout(page, expect);
  await captureArtifact(page, testInfo, "desktop-config.png");

  await openSection(page, "debug");
  await page.getByRole("button", { name: "support bundle" }).click();
  await page.click("#support-bundle-submit");
  await expect(page.locator("#support-bundle-result")).toContainText("Bundle ready");
  await assertShellLayout(page, expect);
  await captureArtifact(page, testInfo, "desktop-debug.png");

  await openSection(page, "infrastructure");
  await openRouteTab(page, "Infrastructure sections", "providers");
  await expect(page.locator("#providers-list")).toBeVisible();
  await assertShellLayout(page, expect);
  await captureArtifact(page, testInfo, "desktop-infrastructure.png");
});
