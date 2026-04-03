import * as foundation from "./dashboard-core-foundation.js";

Object.assign(globalThis, foundation);

const chat = await import("./dashboard-core-chat.js");

Object.assign(globalThis, chat);

export * from "./dashboard-core-foundation.js";
export * from "./dashboard-core-chat.js";
