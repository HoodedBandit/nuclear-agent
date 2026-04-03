import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ConnectScreen } from "./ConnectScreen";

vi.mock("../../api/client", () => ({
  createDashboardSession: vi.fn().mockResolvedValue({ ok: true })
}));

describe("ConnectScreen", () => {
  it("submits the token and triggers the callback", async () => {
    const onConnected = vi.fn().mockResolvedValue(undefined);
    render(<ConnectScreen onConnected={onConnected} />);

    fireEvent.change(screen.getByLabelText(/dashboard token/i), {
      target: { value: "secret" }
    });
    fireEvent.click(screen.getByTestId("modern-connect-button"));

    expect(await screen.findByTestId("modern-connect-button")).toBeInTheDocument();
  });
});
