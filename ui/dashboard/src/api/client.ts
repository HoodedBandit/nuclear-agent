import type {
  BrowserProviderAuthStartResponse,
  BrowserProviderAuthStatusResponse,
  DashboardBootstrapResponse,
  HealthReport,
  SupportBundleResponse,
  UpdateStatusResponse
} from "./types";

export class DashboardApiError extends Error {
  status?: number;

  constructor(message: string, status?: number) {
    super(message);
    this.name = "DashboardApiError";
    this.status = status;
  }
}

async function apiRequest<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const response = await fetch(path, {
    credentials: "same-origin",
    ...options,
    headers: {
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(options.headers || {})
    }
  });

  if (!response.ok) {
    const text = await response.text();
    throw new DashboardApiError(
      `${response.status} ${response.statusText}${text ? `: ${text}` : ""}`,
      response.status
    );
  }

  if (response.status === 204) {
    return null as T;
  }

  const contentType = response.headers.get("content-type") || "";
  if (contentType.includes("application/json")) {
    return (await response.json()) as T;
  }
  return (await response.text()) as T;
}

export function getJson<T>(path: string) {
  return apiRequest<T>(path);
}

export function postJson<T>(path: string, payload: unknown) {
  return apiRequest<T>(path, {
    method: "POST",
    body: JSON.stringify(payload ?? {})
  });
}

export function putJson<T>(path: string, payload: unknown) {
  return apiRequest<T>(path, {
    method: "PUT",
    body: JSON.stringify(payload ?? {})
  });
}

export function deleteJson<T>(path: string) {
  return apiRequest<T>(path, { method: "DELETE" });
}

export function fetchBootstrap() {
  return getJson<DashboardBootstrapResponse>("/v1/dashboard/bootstrap");
}

export function fetchDoctor() {
  return getJson<HealthReport>("/v1/doctor");
}

export function createDashboardSession(token: string) {
  return apiRequest<void>("/auth/dashboard/session", {
    method: "POST",
    body: JSON.stringify({ token })
  });
}

export function clearDashboardSession() {
  return apiRequest<void>("/auth/dashboard/session", {
    method: "DELETE"
  });
}

export function startProviderBrowserAuth(payload: {
  kind: "codex" | "claude";
  provider_id: string;
  display_name: string;
  default_model?: string | null;
  alias_name?: string | null;
  alias_model?: string | null;
  alias_description?: string | null;
  set_as_main?: boolean;
}) {
  return postJson<BrowserProviderAuthStartResponse>(
    "/v1/provider-auth/start",
    payload
  );
}

export function fetchProviderBrowserAuthSession(sessionId: string) {
  return getJson<BrowserProviderAuthStatusResponse>(
    `/v1/provider-auth/${encodeURIComponent(sessionId)}`
  );
}

export function createSupportBundle(payload: {
  output_dir?: string | null;
  log_limit: number;
  session_limit: number;
}) {
  return postJson<SupportBundleResponse>("/v1/support-bundle", payload);
}

export function fetchUpdateStatus() {
  return getJson<UpdateStatusResponse>("/v1/update/status");
}

export function runUpdate(payload?: { wait_for_pid?: number | null }) {
  return postJson<UpdateStatusResponse>("/v1/update/run", payload ?? {});
}
