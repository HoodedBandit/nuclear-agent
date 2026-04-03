import type { ReactNode } from "react";

import type { DashboardDataContextValue } from "./dashboardDataContextInternal";
import { dashboardDataContext } from "./dashboardDataContextInternal";

export function DashboardDataProvider({
  value,
  children
}: {
  value: DashboardDataContextValue;
  children: ReactNode;
}) {
  return <dashboardDataContext.Provider value={value}>{children}</dashboardDataContext.Provider>;
}
