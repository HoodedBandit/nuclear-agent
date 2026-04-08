import {
  createHashRouter,
  RouterProvider
} from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { clearDashboardSession, DashboardApiError, fetchBootstrap } from "../api/client";
import { ConnectScreen } from "../features/auth/ConnectScreen";
import { ChatPage } from "../features/chat/ChatPage";
import { IntegrationsPage } from "../features/integrations/IntegrationsPage";
import { CockpitLayout } from "../features/layout/CockpitLayout";
import { OperationsPage } from "../features/operations/OperationsPage";
import { OverviewPage } from "../features/overview/OverviewPage";
import { SystemPage } from "../features/system/SystemPage";
import { DashboardDataProvider } from "./DashboardDataContext";

const router = createHashRouter([
  {
    path: "/",
    element: <CockpitLayout />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: "chat", element: <ChatPage /> },
      { path: "integrations", element: <IntegrationsPage /> },
      { path: "operations", element: <OperationsPage /> },
      { path: "system", element: <SystemPage /> }
    ]
  }
]);

function DashboardRoot() {
  const queryClient = useQueryClient();
  const bootstrapQuery = useQuery({
    queryKey: ["bootstrap"],
    queryFn: fetchBootstrap,
    refetchInterval: 15_000
  });

  if (bootstrapQuery.isLoading) {
    return <div className="app-loading">Loading cockpit...</div>;
  }

  if (bootstrapQuery.error instanceof DashboardApiError && bootstrapQuery.error.status === 401) {
    return (
      <ConnectScreen
        onConnected={async () => {
          await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
        }}
      />
    );
  }

  if (bootstrapQuery.error) {
    return (
      <main className="app-error">
        <h1>Modern dashboard failed to load</h1>
        <p>
          {bootstrapQuery.error instanceof Error
            ? bootstrapQuery.error.message
            : "Unknown dashboard error."}
        </p>
        <button
          type="button"
          onClick={() => {
            void bootstrapQuery.refetch();
          }}
        >
          Retry dashboard load
        </button>
      </main>
    );
  }

  const bootstrap = bootstrapQuery.data;
  if (!bootstrap) {
    return <div className="app-loading">Loading cockpit...</div>;
  }

  return (
    <DashboardDataProvider
      value={{
        bootstrap,
        onLogout: async () => {
          await clearDashboardSession();
          await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
        }
      }}
    >
      <RouterProvider router={router} />
    </DashboardDataProvider>
  );
}

export function App() {
  return <DashboardRoot />;
}
