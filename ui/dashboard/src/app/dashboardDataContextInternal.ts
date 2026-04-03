import { createContext } from "react";

import type { DashboardBootstrapResponse } from "../api/types";

export interface DashboardDataContextValue {
  bootstrap: DashboardBootstrapResponse;
  onLogout: () => Promise<void> | void;
}

export const dashboardDataContext = createContext<DashboardDataContextValue | null>(null);
