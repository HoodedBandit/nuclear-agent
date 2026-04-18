import { describe, expect, it } from "vitest";
import { itemForPath } from "./navigation";

describe("navigation", () => {
  it("maps compatibility routes onto the current grouped navigation", () => {
    expect(itemForPath("/").key).toBe("overview");
    expect(itemForPath("/integrations").key).toBe("infrastructure");
    expect(itemForPath("/operations").key).toBe("automation");
    expect(itemForPath("/system").key).toBe("config");
  });
});
