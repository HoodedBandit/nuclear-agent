import {
  DashboardApiError
} from "./client";
import type {
  DaemonStatus,
  LogEntry,
  RunTaskRequest,
  RunTaskResponse,
  RunTaskStreamEvent
} from "./types";

const CONTROL_PROTOCOL_VERSION = 2;

type ControlSubscriptionTopic = "status" | "logs";

interface ControlSubscriptionRequest {
  topic: ControlSubscriptionTopic;
  after?: string;
  limit?: number;
}

interface ControlConnected {
  protocol_version: number;
  subscriptions: ControlSubscriptionRequest[];
}

interface ControlLogBatch {
  entries: LogEntry[];
  next_cursor?: string | null;
}

interface ControlTaskStreamEvent {
  request_id: string;
  event: RunTaskStreamEvent;
}

type ControlRequest =
  | { kind: "status" }
  | { kind: "run_task"; payload: { request: RunTaskRequest } };

type ControlResponse =
  | { kind: "status"; payload: DaemonStatus }
  | { kind: "run_task"; payload: RunTaskResponse };

type ControlEvent =
  | { kind: "status"; payload: DaemonStatus }
  | { kind: "logs"; payload: ControlLogBatch }
  | { kind: "task_stream"; payload: ControlTaskStreamEvent };

type ControlServerMessage =
  | { type: "connected"; connection: ControlConnected }
  | { type: "response"; request_id: string; response: ControlResponse }
  | { type: "event"; event: ControlEvent }
  | { type: "error"; request_id?: string | null; error: { message: string; status_code?: number | null } }
  | { type: "pong" };

interface PendingRequest {
  resolve: (value: ControlResponse) => void;
  reject: (error: Error) => void;
  onEvent?: (event: RunTaskStreamEvent) => void;
}

type StatusListener = (status: DaemonStatus) => void;
type LogListener = (batch: ControlLogBatch) => void;

let socket: WebSocket | null = null;
let socketPromise: Promise<WebSocket> | null = null;
let socketReady = false;
let socketClosing = false;
let requestCounter = 0;
const pendingRequests = new Map<string, PendingRequest>();
const statusListeners = new Set<StatusListener>();
const logListeners = new Set<LogListener>();

function controlSocketUrl() {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/v1/ws`;
}

function connectPayload() {
  return {
    type: "connect",
    request: {
      protocol_version: CONTROL_PROTOCOL_VERSION,
      client_name: "modern-dashboard",
      subscriptions: [
        { topic: "status", limit: 1 },
        { topic: "logs", limit: 50 }
      ]
    }
  };
}

function rejectPendingRequests(message: string) {
  for (const pending of pendingRequests.values()) {
    const error = new DashboardApiError(message, 500);
    pending.reject(error);
  }
  pendingRequests.clear();
}

function closeSocket({ rejectMessage }: { rejectMessage?: string } = {}) {
  if (rejectMessage) {
    rejectPendingRequests(rejectMessage);
  }
  if (socket) {
    socketClosing = true;
    try {
      socket.close();
    } catch {
      // Ignore close errors from a dead socket.
    }
  }
  socket = null;
  socketReady = false;
  socketPromise = null;
}

function handleServerMessage(message: ControlServerMessage) {
  if (message.type === "connected") {
    socketReady = true;
    return;
  }

  if (message.type === "response") {
    const pending = pendingRequests.get(message.request_id);
    if (!pending) {
      return;
    }
    pendingRequests.delete(message.request_id);
    pending.resolve(message.response);
    return;
  }

  if (message.type === "error") {
    const pending = message.request_id ? pendingRequests.get(message.request_id) : null;
    const error = new DashboardApiError(
      message.error.message || "Control request failed.",
      message.error.status_code ?? 500
    );
    if (pending && message.request_id) {
      pendingRequests.delete(message.request_id);
      pending.reject(error);
    }
    return;
  }

  if (message.type !== "event") {
    return;
  }

  switch (message.event.kind) {
    case "status":
      statusListeners.forEach((listener) => listener(message.event.payload as DaemonStatus));
      return;
    case "logs":
      logListeners.forEach((listener) => listener(message.event.payload as ControlLogBatch));
      return;
    case "task_stream": {
      const pending = pendingRequests.get(message.event.payload.request_id);
      pending?.onEvent?.(message.event.payload.event);
      return;
    }
  }
}

export async function ensureControlSocket(): Promise<WebSocket> {
  if (socketReady && socket) {
    return socket;
  }
  if (socketPromise) {
    return socketPromise;
  }

  socketPromise = new Promise<WebSocket>((resolve, reject) => {
    let settled = false;
    const nextSocket = new WebSocket(controlSocketUrl());

    nextSocket.addEventListener("open", () => {
      socketClosing = false;
      socket = nextSocket;
      nextSocket.send(JSON.stringify(connectPayload()));
    });

    nextSocket.addEventListener("message", (event) => {
      let parsed: ControlServerMessage;
      try {
        parsed = JSON.parse(String(event.data)) as ControlServerMessage;
      } catch (error) {
        if (!settled) {
          settled = true;
          reject(
            new DashboardApiError(
              error instanceof Error ? error.message : "Control socket parse failed.",
              500
            )
          );
        }
        return;
      }

      if (parsed.type === "connected" && !settled) {
        settled = true;
        resolve(nextSocket);
      }

      handleServerMessage(parsed);
    });

    nextSocket.addEventListener("error", () => {
      if (!settled) {
        settled = true;
        reject(new DashboardApiError("Control socket connection failed.", 500));
      }
    });

    nextSocket.addEventListener("close", () => {
      const intentional = socketClosing;
      socketClosing = false;
      socket = null;
      socketReady = false;
      socketPromise = null;
      rejectPendingRequests("Control socket disconnected.");
      if (!settled) {
        settled = true;
        reject(new DashboardApiError("Control socket closed before it was ready.", 500));
      } else if (!intentional) {
        // Keep the error implicit for runtime callers; they already receive a rejection.
      }
    });
  });

  return socketPromise;
}

async function controlRequest(
  request: ControlRequest,
  options: { onEvent?: (event: RunTaskStreamEvent) => void } = {}
): Promise<ControlResponse> {
  const activeSocket = await ensureControlSocket();
  if (activeSocket.readyState !== WebSocket.OPEN) {
    throw new DashboardApiError("Control socket is not connected.", 500);
  }

  const requestId = `control-${Date.now()}-${++requestCounter}`;
  return new Promise<ControlResponse>((resolve, reject) => {
    pendingRequests.set(requestId, {
      resolve,
      reject,
      onEvent: options.onEvent
    });

    try {
      activeSocket.send(
        JSON.stringify({
          type: "request",
          request_id: requestId,
          request
        })
      );
    } catch (error) {
      pendingRequests.delete(requestId);
      reject(
        error instanceof Error
          ? error
          : new DashboardApiError("Control socket send failed.", 500)
      );
    }
  });
}

export async function controlRunTask(
  payload: RunTaskRequest,
  onEvent: (event: RunTaskStreamEvent) => void
): Promise<RunTaskResponse> {
  const response = await controlRequest(
    {
      kind: "run_task",
      payload: { request: payload }
    },
    { onEvent }
  );

  if (!response || response.kind !== "run_task") {
    throw new DashboardApiError("Unexpected control response kind.", 500);
  }

  return response.payload;
}

export async function fetchLiveStatus(): Promise<DaemonStatus> {
  const response = await controlRequest({ kind: "status" });
  if (!response || response.kind !== "status") {
    throw new DashboardApiError("Unexpected control response kind.", 500);
  }
  return response.payload;
}

export function subscribeControlStatus(listener: StatusListener) {
  statusListeners.add(listener);
  return () => {
    statusListeners.delete(listener);
  };
}

export function subscribeControlLogs(listener: LogListener) {
  logListeners.add(listener);
  return () => {
    logListeners.delete(listener);
  };
}

export function dropControlSocketForDebug() {
  closeSocket({ rejectMessage: "Control socket disconnected." });
}

if (typeof window !== "undefined") {
  const debugWindow = window as Window & {
    nuclearDashboardDebug?: {
      dropControlSocket: () => void;
      getPendingControlRequestCount?: () => number;
    };
  };
  debugWindow.nuclearDashboardDebug = {
    ...(debugWindow.nuclearDashboardDebug ?? {}),
    dropControlSocket: dropControlSocketForDebug,
    getPendingControlRequestCount: () => pendingRequests.size
  };
}
