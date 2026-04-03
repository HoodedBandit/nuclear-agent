import { useContext } from "react";

import { dashboardDataContext } from "./dashboardDataContextInternal";

export function useDashboardData() {
  const value = useContext(dashboardDataContext);
  if (!value) {
    throw new Error("Dashboard data context is unavailable.");
  }
  return value;
}
