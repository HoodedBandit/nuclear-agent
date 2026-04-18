import {
  QueryClient,
  QueryClientProvider,
  useQuery,
  useQueryClient
} from "@tanstack/react-query";
import { useEffect } from "react";
import {
  createHashRouter,
  Navigate,
  RouterProvider
} from "react-router-dom";
import { clearDashboardSession, DashboardApiError, fetchBootstrap } from "../api/client";
import { AutomationPage } from "../features/automation/AutomationPage";
import { ConnectScreen } from "../features/auth/ConnectScreen";
import { ChannelsPage } from "../features/channels/ChannelsPage";
import { ChatPage } from "../features/chat/ChatPage";
import { ConfigPage } from "../features/config/ConfigPage";
import { DebugPage } from "../features/debug/DebugPage";
import { InfrastructurePage } from "../features/infrastructure/InfrastructurePage";
import { AppShell } from "../features/layout/AppShell";
import { LogsPage } from "../features/logs/LogsPage";
import { OverviewPage } from "../features/overview/OverviewPage";
import { SessionsPage } from "../features/sessions/SessionsPage";
import { SkillsPage } from "../features/skills/SkillsPage";
import {
  clearPendingUpdate,
  hasPendingUpdate
} from "../features/system/update-session";
import { DashboardDataProvider } from "./DashboardDataContext";

const router = createHashRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <Navigate to="/overview" replace /> },
      { path: "overview", element: <OverviewPage /> },
      { path: "chat", element: <ChatPage /> },
      { path: "channels", element: <ChannelsPage /> },
      { path: "sessions", element: <SessionsPage /> },
      { path: "logs", element: <LogsPage /> },
      { path: "automation", element: <AutomationPage /> },
      { path: "skills", element: <SkillsPage /> },
      { path: "infrastructure", element: <InfrastructurePage /> },
      { path: "config", element: <ConfigPage /> },
      { path: "debug", element: <DebugPage /> },
      { path: "integrations", element: <Navigate to="/infrastructure" replace /> },
      { path: "operations", element: <Navigate to="/automation" replace /> },
      { path: "system", element: <Navigate to="/config" replace /> }
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
  const pendingUpdate = hasPendingUpdate();

  useEffect(() => {
    if (bootstrapQuery.data && pendingUpdate) {
      clearPendingUpdate();
    }
  }, [bootstrapQuery.data, pendingUpdate]);

  useEffect(() => {
    if (!pendingUpdate || bootstrapQuery.data || !bootstrapQuery.error) {
      return;
    }
    const timer = window.setInterval(() => {
      void bootstrapQuery.refetch();
    }, 1500);
    return () => {
      window.clearInterval(timer);
    };
  }, [bootstrapQuery, pendingUpdate]);

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
    if (pendingUpdate) {
      return (
        <main className="app-loading">
          <h1>Applying update</h1>
          <p>Waiting for the daemon restart.</p>
        </main>
      );
    }
    return (
      <main className="app-error">
        <h1>Bootstrap failed</h1>
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
