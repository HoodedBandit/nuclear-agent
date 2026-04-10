import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createDashboardSession } from "../../api/client";
import { ConnectScreen } from "./ConnectScreen";

vi.mock("../../api/client", () => ({
  createDashboardSession: vi.fn()
}));

describe("ConnectScreen", () => {
  it("creates a session and calls the connected callback", async () => {
    const onConnected = vi.fn(async () => undefined);
    vi.mocked(createDashboardSession).mockResolvedValue(undefined);

    render(<ConnectScreen onConnected={onConnected} />);

    fireEvent.change(screen.getByPlaceholderText("Paste daemon token"), {
      target: { value: "daemon-secret" }
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    await waitFor(() => {
      expect(createDashboardSession).toHaveBeenCalledWith("daemon-secret");
      expect(onConnected).toHaveBeenCalledTimes(1);
    });
  });

  it("renders a helpful error when connection fails", async () => {
    vi.mocked(createDashboardSession).mockRejectedValue(new Error("401 Unauthorized"));

    render(<ConnectScreen onConnected={vi.fn(async () => undefined)} />);

    fireEvent.change(screen.getByPlaceholderText("Paste daemon token"), {
      target: { value: "bad-token" }
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    await screen.findByText("401 Unauthorized");
  });
});
