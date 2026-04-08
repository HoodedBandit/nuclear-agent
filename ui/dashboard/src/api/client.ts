import {
  DashboardBootstrapResponse,
  DashboardSessionRequest,
  ProviderConfig,
  ProviderDiscoveryResponse,
  ProviderReadinessResult,
  ProviderUpsertRequest,
  RunTaskRequest,
  RunTaskResponse,
  RunTaskStreamEvent,
  SessionResumePacket,
  SessionSummary,
  SessionTranscript
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

export async function apiRequest<T>(
  path: string,
  init?: RequestInit
): Promise<T> {
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

export function createDashboardSession(payload: DashboardSessionRequest) {
  return apiRequest<{ ok: boolean }>("/auth/dashboard/session", {
    method: "POST",
    body: JSON.stringify(payload)
  });
}

export function clearDashboardSession() {
  return apiRequest<void>("/auth/dashboard/session", {
    method: "DELETE"
  });
}

export function fetchBootstrap() {
  return apiRequest<DashboardBootstrapResponse>("/v1/dashboard/bootstrap");
}

export function listSessions() {
  return apiRequest<SessionSummary[]>("/v1/sessions");
}

export function fetchSessionTranscript(sessionId: string) {
  return apiRequest<SessionTranscript>(`/v1/sessions/${sessionId}`);
}

export function fetchSessionResumePacket(sessionId: string) {
  return apiRequest<SessionResumePacket>(`/v1/sessions/${sessionId}/resume-packet`);
}

export function listProviders() {
  return apiRequest<ProviderConfig[]>("/v1/providers");
}

export function saveProvider(payload: ProviderUpsertRequest) {
  return apiRequest<void>("/v1/providers", {
    method: "POST",
    body: JSON.stringify(payload)
  });
}

export function discoverProviderModels(payload: ProviderUpsertRequest) {
  return apiRequest<string[]>("/v1/providers/discover-models", {
    method: "POST",
    body: JSON.stringify(payload)
  });
}

export function discoverProvider(payload: ProviderUpsertRequest) {
  return apiRequest<ProviderDiscoveryResponse>("/v1/providers/discover", {
    method: "POST",
    body: JSON.stringify(payload)
  });
}

export function validateProvider(payload: ProviderUpsertRequest) {
  return apiRequest<ProviderReadinessResult>("/v1/providers/validate", {
    method: "POST",
    body: JSON.stringify(payload)
  });
}

export async function streamRunTask(
  payload: RunTaskRequest,
  onEvent: (event: RunTaskStreamEvent) => void
): Promise<RunTaskResponse> {
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
  let completed: RunTaskResponse | null = null;

  while (true) {
    const { value, done } = await reader.read();
    buffered += decoder.decode(value ?? new Uint8Array(), { stream: !done });

    let newlineIndex = buffered.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = buffered.slice(0, newlineIndex).trim();
      buffered = buffered.slice(newlineIndex + 1);
      if (line) {
        const event = JSON.parse(line) as RunTaskStreamEvent;
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
