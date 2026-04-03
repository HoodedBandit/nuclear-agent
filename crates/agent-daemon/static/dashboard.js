import * as dashboardCore from "./dashboard-core.js";

Object.assign(globalThis, dashboardCore);

const dashboardControl = await import("./dashboard-control.js");
await import("./dashboard-connectors.js");
await import("./dashboard-providers.js");
await import("./dashboard-plugins.js");
await import("./dashboard-workspace.js");
await import("./dashboard-settings.js");
const dashboardChat = await import("./dashboard-chat.js");

Object.assign(globalThis, dashboardControl);
Object.assign(globalThis, dashboardChat);

window.dashboardConnectors?.bind?.();
window.dashboardProviders?.bind?.();
window.dashboardPlugins?.bind?.();
window.dashboardWorkspace?.bind?.();
window.dashboardSettings?.bind?.();

dashboardCore.bootstrapDashboard();
