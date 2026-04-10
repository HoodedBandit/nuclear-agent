import { createContext, useContext } from "react";
import type { DashboardBootstrapResponse } from "../api/types";

export interface DashboardDataContextValue {
  bootstrap: DashboardBootstrapResponse;
  onLogout: () => Promise<void>;
}

export const DashboardDataContext = createContext<DashboardDataContextValue | null>(
  null
);

export function useDashboardData() {
  const value = useContext(DashboardDataContext);
  if (!value) {
    throw new Error("Dashboard data context is not available.");
  }
  return value;
}
