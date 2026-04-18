const fs = require("fs");
const path = require("path");

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

async function connectDashboard(page, expectFn) {
  const state = readState();
  await page.goto("/ui");
  await page.fill("#token-input", state.token);
  await page.click("#connect-button");
  await expectFn(page.getByTestId("modern-main-workspace")).toBeVisible();
  await expectFn(page.getByTestId("modern-nav-rail")).toContainText("Nuclear");
  await expectFn(page.getByTestId("modern-nav-rail")).toContainText("Parity gaps");
  return state;
}

async function openSection(page, name) {
  const locator = page.getByTestId(`nav-${name}`);
  try {
    await locator.click({ timeout: 3000 });
  } catch (error) {
    const href = await locator.getAttribute("href");
    if (!href) {
      throw error;
    }
    await page.goto(href.startsWith("#") ? `/ui${href}` : href);
  }
}

async function openRouteTab(page, ariaLabel, tab) {
  const tabs = page.locator(`section[aria-label="${ariaLabel}"]`);
  await tabs.getByRole("button", { name: tab, exact: true }).click();
}

async function openDisclosure(page, title) {
  await page.locator("summary", { hasText: title }).click();
}

async function assertShellLayout(page, expect) {
  const shellReport = await page.evaluate(() => {
    const doc = document.documentElement;
    const body = document.body;
    const topbar = document.querySelector(".topbar");
    const content = document.querySelector(".content");
    const nav = document.querySelector(".shell-nav");

    const topbarBox = topbar?.getBoundingClientRect() ?? null;
    const contentBox = content?.getBoundingClientRect() ?? null;
    const navBox = nav?.getBoundingClientRect() ?? null;

    return {
      docOverflow: Math.max(0, doc.scrollWidth - doc.clientWidth),
      bodyOverflow: Math.max(0, body.scrollWidth - body.clientWidth),
      topbarBottom: topbarBox ? topbarBox.bottom : null,
      contentTop: contentBox ? contentBox.top : null,
      navRight: navBox ? navBox.right : null,
      contentLeft: contentBox ? contentBox.left : null
    };
  });

  expect(shellReport.docOverflow).toBeLessThanOrEqual(1);
  expect(shellReport.bodyOverflow).toBeLessThanOrEqual(1);
  if (
    typeof shellReport.topbarBottom === "number" &&
    typeof shellReport.contentTop === "number"
  ) {
    expect(shellReport.contentTop).toBeGreaterThanOrEqual(shellReport.topbarBottom - 1);
  }
  if (
    typeof shellReport.navRight === "number" &&
    typeof shellReport.contentLeft === "number" &&
    shellReport.navRight > 0
  ) {
    expect(shellReport.contentLeft).toBeGreaterThanOrEqual(shellReport.navRight - 1);
  }
}

const VIEWPORTS = {
  desktop: { width: 1440, height: 900 },
  tablet: { width: 1024, height: 768 },
  mobile: { width: 390, height: 844 }
};

module.exports = {
  VIEWPORTS,
  assertShellLayout,
  connectDashboard,
  openDisclosure,
  openRouteTab,
  openSection,
  readState
};
