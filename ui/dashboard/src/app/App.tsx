import {
  QueryClient,
  QueryClientProvider,
  useQuery,
  useQueryClient
} from "@tanstack/react-query";
import { createHashRouter, RouterProvider } from "react-router-dom";
import { clearDashboardSession, DashboardApiError, fetchBootstrap } from "../api/client";
import { ConnectScreen } from "../features/auth/ConnectScreen";
import { ChatPage } from "../features/chat/ChatPage";
import { IntegrationsPage } from "../features/integrations/IntegrationsPage";
import { AppShell } from "../features/layout/AppShell";
import { OperationsPage } from "../features/operations/OperationsPage";
import { OverviewPage } from "../features/overview/OverviewPage";
import { SystemPage } from "../features/system/SystemPage";
import { DashboardDataProvider } from "./DashboardDataContext";

const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: "chat", element: <ChatPage /> },
      { path: "operations", element: <OperationsPage /> },
      { path: "integrations", element: <IntegrationsPage /> },
      { path: "system", element: <SystemPage /> }
    ]
  }
]);

const queryClient = new QueryClient();

function AppRoot() {
  const client = useQueryClient();
  const bootstrapQuery = useQuery({
    queryKey: ["bootstrap"],
    queryFn: fetchBootstrap,
    refetchInterval: 12000
  });

  if (bootstrapQuery.isLoading) {
    return <div className="app-loading">Loading cockpit...</div>;
  }

  if (bootstrapQuery.error instanceof DashboardApiError && bootstrapQuery.error.status === 401) {
    return (
      <ConnectScreen
        onConnected={async () => {
          await client.invalidateQueries({ queryKey: ["bootstrap"] });
        }}
      />
    );
  }

  if (bootstrapQuery.error) {
    return (
      <main className="app-error">
        <h1>Dashboard bootstrap failed</h1>
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
          Retry
        </button>
      </main>
    );
  }

  if (!bootstrapQuery.data) {
    return <div className="app-loading">Loading cockpit...</div>;
  }

  return (
    <DashboardDataProvider
      bootstrap={bootstrapQuery.data}
      onLogout={async () => {
        await clearDashboardSession();
        await client.invalidateQueries({ queryKey: ["bootstrap"] });
      }}
    >
      <RouterProvider router={router} />
    </DashboardDataProvider>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AppRoot />
    </QueryClientProvider>
  );
}
