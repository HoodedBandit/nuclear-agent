import type {
  AppConnectorConfig,
  AliasUpsertRequest,
  AutonomyEnableRequest,
  AutonomyProfile,
  AutopilotConfig,
  AutopilotUpdateRequest,
  BraveConnectorConfig,
  BrowserProviderAuthStartRequest,
  BrowserProviderAuthStartResponse,
  BrowserProviderAuthStatusResponse,
  ConnectorApprovalRecord,
  DaemonConfigUpdateRequest,
  DashboardBootstrapResponse,
  DashboardSessionRequest,
  DiscordConnectorConfig,
  GmailConnectorConfig,
  HealthReport,
  HomeAssistantConnectorConfig,
  InboxConnectorConfig,
  InstalledPluginConfig,
  McpServerConfig,
  McpServerUpsertRequest,
  MemoryRecord,
  MemoryRebuildRequest,
  MemoryRebuildResponse,
  MemoryReviewUpdateRequest,
  MemorySearchQuery,
  MemorySearchResponse,
  MemoryUpsertRequest,
  Mission,
  MissionCheckpoint,
  MissionControlRequest,
  ModelAlias,
  PermissionPreset,
  PermissionUpdateRequest,
  PluginDoctorReport,
  PluginInstallRequest,
  PluginStateUpdateRequest,
  PluginUpdateRequest,
  ProviderConfig,
  ProviderDiscoveryResponse,
  ProviderReadinessResult,
  ProviderSuggestionRequest,
  ProviderSuggestionResponse,
  ProviderUpsertRequest,
  RunTaskRequest,
  SessionCompactRequest,
  SessionForkRequest,
  SessionMutationResponse,
  SessionRenameRequest,
  SessionResumePacket,
  SessionSummary,
  SessionTranscript,
  SignalConnectorConfig,
  SlackConnectorConfig,
  TelegramConnectorConfig,
  TrustPolicy,
  TrustUpdateRequest,
  WebhookConnectorConfig,
  WorkspaceActionRequest,
  WorkspaceDiffResponse,
  WorkspaceInspectRequest,
  WorkspaceInspectResponse,
  WorkspaceInitResponse,
  WorkspaceShellResponse
} from "./types";

export class DashboardApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "DashboardApiError";
    this.status = status;
  }
}

async function parseError(response: Response): Promise<DashboardApiError> {
  let message = response.statusText || "Request failed";
  try {
    const payload = (await response.clone().json()) as { error?: string; message?: string };
    message = payload.error ?? payload.message ?? message;
  } catch {
    const text = await response.text();
    if (text.trim()) {
      message = text.trim();
    }
  }
  return new DashboardApiError(message, response.status);
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {})
    },
    ...init
  });

  if (!response.ok) {
    throw await parseError(response);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export function apiGet<T>(path: string) {
  return request<T>(path);
}

export function apiPost<T>(path: string, body: unknown) {
  return request<T>(path, {
    method: "POST",
    body: JSON.stringify(body)
  });
}

export function apiPut<T>(path: string, body: unknown) {
  return request<T>(path, {
    method: "PUT",
    body: JSON.stringify(body)
  });
}

export function apiDelete<T>(path: string) {
  return request<T>(path, {
    method: "DELETE"
  });
}

export function createDashboardSession(payload: DashboardSessionRequest) {
  return apiPost<{ ok: boolean }>("/auth/dashboard/session", payload);
}

export function clearDashboardSession() {
  return apiDelete<void>("/auth/dashboard/session");
}

export function fetchBootstrap() {
  return apiGet<DashboardBootstrapResponse>("/v1/dashboard/bootstrap");
}

export function fetchConfig() {
  return apiGet<Record<string, unknown>>("/v1/config");
}

export function saveConfig(config: Record<string, unknown>) {
  return apiPut<Record<string, unknown>>("/v1/config", config);
}

export function fetchDoctorReport() {
  return apiGet<HealthReport>("/v1/doctor");
}

export function listSessions(limit = 25) {
  return apiGet<SessionSummary[]>(`/v1/sessions?limit=${limit}`);
}

export function fetchSessionTranscript(sessionId: string) {
  return apiGet<SessionTranscript>(`/v1/sessions/${sessionId}`);
}

export function fetchSessionResumePacket(sessionId: string) {
  return apiGet<SessionResumePacket>(`/v1/sessions/${sessionId}/resume-packet`);
}

export function renameSession(sessionId: string, payload: SessionRenameRequest) {
  return apiPut<{ ok: boolean; title: string }>(`/v1/sessions/${sessionId}/title`, payload);
}

export function forkSession(sessionId: string, payload: SessionForkRequest) {
  return apiPost<SessionMutationResponse>(`/v1/sessions/${sessionId}/fork`, payload);
}

export function compactSession(sessionId: string, payload: SessionCompactRequest) {
  return apiPost<SessionMutationResponse>(`/v1/sessions/${sessionId}/compact`, payload);
}

export function listProviders() {
  return apiGet<ProviderConfig[]>("/v1/providers");
}

export function saveProvider(payload: ProviderUpsertRequest) {
  return apiPost<ProviderConfig>("/v1/providers", payload);
}

export function deleteProvider(providerId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/providers/${providerId}`);
}

export function clearProviderCredentials(providerId: string) {
  return apiDelete<ProviderConfig>(`/v1/providers/${providerId}/credentials`);
}

export function discoverProviderModels(payload: ProviderUpsertRequest) {
  return apiPost<string[]>("/v1/providers/discover-models", payload);
}

export function discoverProvider(payload: ProviderUpsertRequest) {
  return apiPost<ProviderDiscoveryResponse>("/v1/providers/discover", payload);
}

export function validateProvider(payload: ProviderUpsertRequest) {
  return apiPost<ProviderReadinessResult>("/v1/providers/validate", payload);
}

export function suggestProviderDefaults(payload: ProviderSuggestionRequest) {
  return apiPost<ProviderSuggestionResponse>("/v1/providers/suggest", payload);
}

export function listAliases() {
  return apiGet<ModelAlias[]>("/v1/aliases");
}

export function saveAlias(payload: AliasUpsertRequest) {
  return apiPost<ModelAlias>("/v1/aliases", payload);
}

export function deleteAlias(aliasName: string) {
  return apiDelete<{ ok: boolean }>(`/v1/aliases/${encodeURIComponent(aliasName)}`);
}

export function updateMainAlias(alias: string) {
  return apiPut<{ alias: string; provider_id: string; model: string }>("/v1/main-alias", { alias });
}

export function startProviderBrowserAuth(payload: BrowserProviderAuthStartRequest) {
  return apiPost<BrowserProviderAuthStartResponse>("/v1/provider-auth/start", payload);
}

export function fetchProviderBrowserAuthStatus(sessionId: string) {
  return apiGet<BrowserProviderAuthStatusResponse>(`/v1/provider-auth/${sessionId}`);
}

export function listTelegramConnectors() {
  return apiGet<TelegramConnectorConfig[]>("/v1/telegram");
}

export function listAppConnectors() {
  return apiGet<AppConnectorConfig[]>("/v1/apps");
}

export function saveAppConnector(payload: { connector: AppConnectorConfig }) {
  return apiPost<AppConnectorConfig>("/v1/apps", payload);
}

export function deleteAppConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/apps/${connectorId}`);
}

export function saveTelegramConnector(payload: {
  connector: TelegramConnectorConfig;
  bot_token?: string | null;
}) {
  return apiPost<TelegramConnectorConfig>("/v1/telegram", payload);
}

export function deleteTelegramConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/telegram/${connectorId}`);
}

export function listDiscordConnectors() {
  return apiGet<DiscordConnectorConfig[]>("/v1/discord");
}

export function saveDiscordConnector(payload: {
  connector: DiscordConnectorConfig;
  bot_token?: string | null;
}) {
  return apiPost<DiscordConnectorConfig>("/v1/discord", payload);
}

export function deleteDiscordConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/discord/${connectorId}`);
}

export function listSlackConnectors() {
  return apiGet<SlackConnectorConfig[]>("/v1/slack");
}

export function saveSlackConnector(payload: {
  connector: SlackConnectorConfig;
  bot_token?: string | null;
}) {
  return apiPost<SlackConnectorConfig>("/v1/slack", payload);
}

export function deleteSlackConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/slack/${connectorId}`);
}

export function listSignalConnectors() {
  return apiGet<SignalConnectorConfig[]>("/v1/signal");
}

export function saveSignalConnector(payload: { connector: SignalConnectorConfig }) {
  return apiPost<SignalConnectorConfig>("/v1/signal", payload);
}

export function deleteSignalConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/signal/${connectorId}`);
}

export function listHomeAssistantConnectors() {
  return apiGet<HomeAssistantConnectorConfig[]>("/v1/home-assistant");
}

export function saveHomeAssistantConnector(payload: {
  connector: HomeAssistantConnectorConfig;
  access_token?: string | null;
}) {
  return apiPost<HomeAssistantConnectorConfig>("/v1/home-assistant", payload);
}

export function deleteHomeAssistantConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/home-assistant/${connectorId}`);
}

export function listInboxConnectors() {
  return apiGet<InboxConnectorConfig[]>("/v1/inboxes");
}

export function saveInboxConnector(payload: { connector: InboxConnectorConfig }) {
  return apiPost<InboxConnectorConfig>("/v1/inboxes", payload);
}

export function deleteInboxConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/inboxes/${connectorId}`);
}

export function listWebhookConnectors() {
  return apiGet<WebhookConnectorConfig[]>("/v1/webhooks");
}

export function saveWebhookConnector(payload: {
  connector: WebhookConnectorConfig;
  webhook_token?: string | null;
  clear_webhook_token?: boolean;
}) {
  return apiPost<WebhookConnectorConfig>("/v1/webhooks", payload);
}

export function deleteWebhookConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/webhooks/${connectorId}`);
}

export function listGmailConnectors() {
  return apiGet<GmailConnectorConfig[]>("/v1/gmail");
}

export function saveGmailConnector(payload: {
  connector: GmailConnectorConfig;
  oauth_token?: string | null;
}) {
  return apiPost<GmailConnectorConfig>("/v1/gmail", payload);
}

export function deleteGmailConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/gmail/${connectorId}`);
}

export function listBraveConnectors() {
  return apiGet<BraveConnectorConfig[]>("/v1/brave");
}

export function saveBraveConnector(payload: {
  connector: BraveConnectorConfig;
  api_key?: string | null;
}) {
  return apiPost<BraveConnectorConfig>("/v1/brave", payload);
}

export function deleteBraveConnector(connectorId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/brave/${connectorId}`);
}

export function listPlugins() {
  return apiGet<InstalledPluginConfig[]>("/v1/plugins");
}

export function installPlugin(payload: PluginInstallRequest) {
  return apiPost<InstalledPluginConfig>("/v1/plugins/install", payload);
}

export function listPluginDoctorReports() {
  return apiGet<PluginDoctorReport[]>("/v1/plugins/doctor");
}

export function getPluginDoctorReport(pluginId: string) {
  return apiGet<PluginDoctorReport>(`/v1/plugins/${pluginId}/doctor`);
}

export function updatePluginState(pluginId: string, payload: PluginStateUpdateRequest) {
  return apiPut<InstalledPluginConfig>(`/v1/plugins/${pluginId}`, payload);
}

export function updatePlugin(pluginId: string, payload: PluginUpdateRequest) {
  return apiPost<InstalledPluginConfig>(`/v1/plugins/${pluginId}/update`, payload);
}

export function deletePlugin(pluginId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/plugins/${pluginId}`);
}

export function listMcpServers() {
  return apiGet<McpServerConfig[]>("/v1/mcp");
}

export function saveMcpServer(payload: McpServerUpsertRequest) {
  return apiPost<McpServerConfig>("/v1/mcp", payload);
}

export function deleteMcpServer(serverId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/mcp/${serverId}`);
}

export function listMemories(limit = 50) {
  return apiGet<MemoryRecord[]>(`/v1/memory?limit=${limit}`);
}

export function listProfileMemories(limit = 25) {
  return apiGet<MemoryRecord[]>(`/v1/memory/profile?limit=${limit}`);
}

export function listMemoryReviewQueue(limit = 50) {
  return apiGet<MemoryRecord[]>(`/v1/memory/review?limit=${limit}`);
}

export function searchMemory(payload: MemorySearchQuery) {
  return apiPost<MemorySearchResponse>("/v1/memory/search", payload);
}

export function rebuildMemory(payload: MemoryRebuildRequest) {
  return apiPost<MemoryRebuildResponse>("/v1/memory/rebuild", payload);
}

export function saveMemory(payload: MemoryUpsertRequest) {
  return apiPost<MemoryRecord>("/v1/memory", payload);
}

export function approveMemory(memoryId: string, payload: MemoryReviewUpdateRequest) {
  return apiPost<MemoryRecord>(`/v1/memory/${memoryId}/approve`, payload);
}

export function rejectMemory(memoryId: string, payload: MemoryReviewUpdateRequest) {
  return apiPost<MemoryRecord>(`/v1/memory/${memoryId}/reject`, payload);
}

export function deleteMemory(memoryId: string) {
  return apiDelete<{ ok: boolean }>(`/v1/memory/${memoryId}`);
}

export function listConnectorApprovals(limit = 50) {
  return apiGet<ConnectorApprovalRecord[]>(`/v1/connector-approvals?limit=${limit}`);
}

export function approveConnectorApproval(approvalId: string, note?: string) {
  return apiPost<ConnectorApprovalRecord>(`/v1/connector-approvals/${approvalId}/approve`, {
    note
  });
}

export function rejectConnectorApproval(approvalId: string, note?: string) {
  return apiPost<ConnectorApprovalRecord>(`/v1/connector-approvals/${approvalId}/reject`, {
    note
  });
}

export function listMissions(limit = 100) {
  return apiGet<Mission[]>(`/v1/missions?limit=${limit}`);
}

export function saveMission(payload: Mission) {
  return apiPost<Mission>("/v1/missions", payload);
}

export function fetchMission(missionId: string) {
  return apiGet<Mission>(`/v1/missions/${missionId}`);
}

export function pauseMission(missionId: string, payload: MissionControlRequest) {
  return apiPost<Mission>(`/v1/missions/${missionId}/pause`, payload);
}

export function resumeMission(missionId: string, payload: MissionControlRequest) {
  return apiPost<Mission>(`/v1/missions/${missionId}/resume`, payload);
}

export function cancelMission(missionId: string) {
  return apiPost<Mission>(`/v1/missions/${missionId}/cancel`, {});
}

export function listMissionCheckpoints(missionId: string, limit = 25) {
  return apiGet<MissionCheckpoint[]>(`/v1/missions/${missionId}/checkpoints?limit=${limit}`);
}

export function listEvents(limit = 50) {
  return apiGet<Array<{ id: string; level: string; scope: string; message: string; created_at: string }>>(
    `/v1/events?limit=${limit}`
  );
}

export function listLogs(limit = 50) {
  return apiGet<Array<{ id: string; level: string; scope: string; message: string; created_at: string }>>(
    `/v1/logs?limit=${limit}`
  );
}

export function getTrust() {
  return apiGet<TrustPolicy>("/v1/trust");
}

export function updateTrust(payload: TrustUpdateRequest) {
  return apiPut<TrustPolicy>("/v1/trust", payload);
}

export function getPermissionPreset() {
  return apiGet<PermissionPreset>("/v1/permissions");
}

export function updatePermissionPreset(payload: PermissionUpdateRequest) {
  return apiPut<PermissionPreset>("/v1/permissions", payload);
}

export function getAutonomyStatus() {
  return apiGet<AutonomyProfile>("/v1/autonomy/status");
}

export function enableAutonomy(payload: AutonomyEnableRequest) {
  return apiPost<AutonomyProfile>("/v1/autonomy/enable", payload);
}

export function pauseAutonomy() {
  return apiPost<AutonomyProfile>("/v1/autonomy/pause", {});
}

export function resumeAutonomy() {
  return apiPost<AutonomyProfile>("/v1/autonomy/resume", {});
}

export function getAutopilotStatus() {
  return apiGet<AutopilotConfig>("/v1/autopilot/status");
}

export function updateAutopilot(payload: AutopilotUpdateRequest) {
  return apiPut<AutopilotConfig>("/v1/autopilot/status", payload);
}

export function updateDaemonConfig(payload: DaemonConfigUpdateRequest) {
  return apiPut<{ persistence_mode: string; auto_start: boolean }>("/v1/daemon/config", payload);
}

export function inspectWorkspace(payload: WorkspaceInspectRequest) {
  return apiPost<WorkspaceInspectResponse>("/v1/workspace/inspect", payload);
}

export function workspaceDiff(payload: WorkspaceActionRequest) {
  return apiPost<WorkspaceDiffResponse>("/v1/workspace/diff", payload);
}

export function workspaceInit(payload: WorkspaceActionRequest) {
  return apiPost<WorkspaceInitResponse>("/v1/workspace/init", payload);
}

export function workspaceShell(command: string, cwd?: string | null) {
  return apiPost<WorkspaceShellResponse>("/v1/workspace/shell", { command, cwd });
}

export async function streamRunTask(
  payload: RunTaskRequest,
  onEvent: (event: import("./types").RunTaskStreamEvent) => void
) {
  const response = await fetch("/v1/run/stream", {
    method: "POST",
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json"
    },
    body: JSON.stringify(payload)
  });

  if (!response.ok || !response.body) {
    throw await parseError(response);
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffered = "";
  let completed: import("./types").RunTaskResponse | null = null;

  while (true) {
    const { value, done } = await reader.read();
    buffered += decoder.decode(value ?? new Uint8Array(), { stream: !done });

    let newlineIndex = buffered.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = buffered.slice(0, newlineIndex).trim();
      buffered = buffered.slice(newlineIndex + 1);
      if (line) {
        const event = JSON.parse(line) as import("./types").RunTaskStreamEvent;
        onEvent(event);
        if (event.type === "completed") {
          completed = event.response;
        }
        if (event.type === "error") {
          throw new DashboardApiError(event.message, 500);
        }
      }
      newlineIndex = buffered.indexOf("\n");
    }

    if (done) {
      break;
    }
  }

  if (!completed) {
    throw new DashboardApiError("Task stream ended before completion.", 500);
  }

  return completed;
}
