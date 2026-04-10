import type { PropsWithChildren } from "react";
import { DashboardDataContext } from "./dashboard-data";
import type { DashboardDataContextValue } from "./dashboard-data";

export function DashboardDataProvider(
  props: PropsWithChildren<DashboardDataContextValue>
) {
  const { children, ...value } = props;
  return (
    <DashboardDataContext.Provider value={value}>
      {children}
    </DashboardDataContext.Provider>
  );
}
