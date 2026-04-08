import { workspaceDiff, workspaceInit, workspaceShell } from "../../api/client";
import type {
  DashboardBootstrapResponse,
  InputAttachment,
  PermissionPreset,
  SessionSummary,
  TaskMode
} from "../../api/types";

export type ChatConsoleTone = "info" | "good" | "warn" | "danger";

export interface ChatConsoleEntry {
  id: string;
  title: string;
  body: string;
  tone: ChatConsoleTone;
}

interface ChatConsoleState {
  bootstrap: DashboardBootstrapResponse;
  sessions: SessionSummary[];
  selectedSession: SessionSummary | null;
  alias: string;
  requestedModel: string;
  taskMode: "" | TaskMode;
  permissionPreset: "" | PermissionPreset;
  attachments: InputAttachment[];
  cwd: string;
}

interface ChatConsoleActions {
  setAlias: (alias: string) => void;
  setRequestedModel: (value: string) => void;
  setTaskMode: (value: "" | TaskMode) => void;
  setPermissionPreset: (value: "" | PermissionPreset) => void;
  addAttachment: (path: string) => void;
  clearAttachments: () => void;
  startNewSession: () => void;
  setCwd: (cwd: string) => void;
}

export interface ChatConsoleContext {
  state: ChatConsoleState;
  actions: ChatConsoleActions;
}

export interface ChatConsoleResult {
  handled: boolean;
  entry?: ChatConsoleEntry;
}

function entry(title: string, body: string, tone: ChatConsoleTone = "info"): ChatConsoleEntry {
  return {
    id: `${title}-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    title,
    body,
    tone
  };
}

function findAliasByName(name: string, bootstrap: DashboardBootstrapResponse) {
  return bootstrap.aliases.find((alias) => alias.alias.toLowerCase() === name.toLowerCase()) ?? null;
}

function preferredAliasForProvider(providerId: string, bootstrap: DashboardBootstrapResponse) {
  const knownAlias =
    bootstrap.aliases.find((alias) => alias.provider_id === providerId)?.alias ?? null;
  if (knownAlias) {
    return knownAlias;
  }
  return bootstrap.status.main_target?.provider_id === providerId
    ? bootstrap.status.main_target.alias
    : null;
}

function resolveProviderAlias(query: string, bootstrap: DashboardBootstrapResponse) {
  const lowered = query.toLowerCase();
  const directAlias = findAliasByName(lowered, bootstrap);
  if (directAlias) {
    return directAlias.alias;
  }

  const matches = bootstrap.providers
    .filter((provider) => {
      const displayName = provider.display_name.toLowerCase();
      return (
        provider.id.toLowerCase() === lowered ||
        displayName === lowered ||
        provider.id.toLowerCase().includes(lowered) ||
        displayName.includes(lowered)
      );
    })
    .map((provider) => preferredAliasForProvider(provider.id, bootstrap))
    .filter((alias): alias is string => Boolean(alias));

  const unique = [...new Set(matches)];
  if (unique.length === 1) {
    return unique[0];
  }
  if (!unique.length) {
    throw new Error(`Unknown logged-in provider "${query}".`);
  }
  throw new Error(`Provider selection "${query}" is ambiguous.`);
}

function formatStatusSummary(state: ChatConsoleState) {
  const { bootstrap, selectedSession, alias, requestedModel, taskMode, permissionPreset, attachments, cwd } = state;
  const status = bootstrap.status;

  return [
    `session=${selectedSession?.id ?? "(new)"}`,
    selectedSession?.title ? `title=${selectedSession.title}` : null,
    `alias=${alias || status.main_agent_alias || "-"}`,
    `model=${requestedModel || selectedSession?.model || "-"}`,
    `mode=${taskMode || selectedSession?.task_mode || "default"}`,
    `permission_preset=${permissionPreset || bootstrap.permissions || "default"}`,
    `attachments=${attachments.length}`,
    `cwd=${cwd || "(daemon cwd)"}`,
    status.main_target
      ? `main=${status.main_target.alias} (${status.main_target.provider_id}/${status.main_target.model})`
      : "main=(not configured)",
    `autonomy=${status.autonomy?.state || "-"}`,
    `autopilot=${status.autopilot?.state || "-"}`,
    `active_missions=${status.active_missions || 0}`,
    `memories=${status.memories || 0}`
  ]
    .filter(Boolean)
    .join("\n");
}

export async function executeChatConsoleInput(
  input: string,
  context: ChatConsoleContext
): Promise<ChatConsoleResult> {
  const line = input.trim();
  if (!line) {
    return { handled: false };
  }

  const { state, actions } = context;

  if (line.startsWith("!")) {
    const command = line.slice(1).trim();
    if (!command) {
      throw new Error("Shell command is empty.");
    }
    const response = await workspaceShell(command, state.cwd || null);
    actions.setCwd(response.cwd);
    return { handled: true, entry: entry(`Shell ${command}`, response.output || "(no output)") };
  }

  if (!line.startsWith("/")) {
    return { handled: false };
  }

  const body = line.slice(1);
  const splitIndex = body.search(/\s/);
  const command = (splitIndex >= 0 ? body.slice(0, splitIndex) : body).trim().toLowerCase();
  const rawArgs = (splitIndex >= 0 ? body.slice(splitIndex + 1) : "").trim();

  switch (command) {
    case "help":
      return {
        handled: true,
        entry: entry(
          "Commands",
          [
            "/help",
            "/status",
            "/alias [name]",
            "/provider [name]",
            "/providers [name]",
            "/model [alias-or-model]",
            "/mode [build|daily|default]",
            "/permissions [suggest|auto_edit|full_auto|default]",
            "/attach <path>",
            "/attachments",
            "/detach",
            "/attachments-clear",
            "/diff",
            "/init",
            "/new",
            "/clear",
            "!<command>"
          ].join("\n")
        )
      };
    case "status":
      return { handled: true, entry: entry("Status", formatStatusSummary(state)) };
    case "alias":
      if (!rawArgs) {
        return {
          handled: true,
          entry: entry(
            "Aliases",
            state.bootstrap.aliases
              .map((alias) => `${alias.alias} -> ${alias.provider_id} / ${alias.model}`)
              .join("\n") || "No aliases configured."
          )
        };
      }
      {
        const aliasMatch = findAliasByName(rawArgs, state.bootstrap);
        if (!aliasMatch) {
          throw new Error(`Unknown alias "${rawArgs}".`);
        }
        actions.setAlias(aliasMatch.alias);
        actions.setRequestedModel("");
        return {
          handled: true,
          entry: entry("Alias", `Switched chat alias to ${aliasMatch.alias}.`, "good")
        };
      }
    case "provider":
    case "providers":
      if (!rawArgs) {
        return {
          handled: true,
          entry: entry(
            "Providers",
            state.bootstrap.providers
              .map((provider) => {
                const alias = preferredAliasForProvider(provider.id, state.bootstrap) ?? "-";
                return `${provider.display_name} (${provider.id}) -> ${alias}`;
              })
              .join("\n") || "No providers configured."
          )
        };
      }
      {
        const aliasName = resolveProviderAlias(rawArgs, state.bootstrap);
        actions.setAlias(aliasName);
        actions.setRequestedModel("");
        return {
          handled: true,
          entry: entry("Providers", `Switched chat alias to ${aliasName}.`, "good")
        };
      }
    case "model":
      if (!rawArgs) {
        return {
          handled: true,
          entry: entry(
            "Model target",
            [
              `alias=${state.alias || "-"}`,
              `requested_model=${state.requestedModel || "(default)"}`,
              ...state.bootstrap.aliases.map((alias) => `${alias.alias} -> ${alias.provider_id} / ${alias.model}`)
            ].join("\n")
          )
        };
      }
      {
        const aliasMatch = findAliasByName(rawArgs, state.bootstrap);
        if (aliasMatch) {
          actions.setAlias(aliasMatch.alias);
          actions.setRequestedModel("");
          return {
            handled: true,
            entry: entry("Model target", `Switched chat alias to ${aliasMatch.alias}.`, "good")
          };
        }
        actions.setRequestedModel(rawArgs);
        return {
          handled: true,
          entry: entry("Model target", `Set explicit model override to ${rawArgs}.`, "good")
        };
      }
    case "mode":
      if (!rawArgs) {
        return {
          handled: true,
          entry: entry("Mode", `mode=${state.taskMode || "default"}`)
        };
      }
      {
        const nextMode = rawArgs.toLowerCase();
        if (!["default", "build", "daily"].includes(nextMode)) {
          throw new Error("usage: /mode [build|daily|default]");
        }
        actions.setTaskMode(nextMode === "default" ? "" : (nextMode as TaskMode));
        return {
          handled: true,
          entry: entry("Mode", `mode=${nextMode === "default" ? "default" : nextMode}`, "good")
        };
      }
    case "permissions":
    case "approvals":
      if (!rawArgs) {
        return {
          handled: true,
          entry: entry(
            "Permissions",
            `permission_preset=${state.permissionPreset || state.bootstrap.permissions || "default"}`
          )
        };
      }
      {
        const nextPermission = rawArgs.toLowerCase();
        if (!["default", "suggest", "auto_edit", "full_auto"].includes(nextPermission)) {
          throw new Error("usage: /permissions [suggest|auto_edit|full_auto|default]");
        }
        actions.setPermissionPreset(
          nextPermission === "default" ? "" : (nextPermission as PermissionPreset)
        );
        return {
          handled: true,
          entry: entry(
            "Permissions",
            `Set chat permission preset to ${nextPermission}.`,
            "good"
          )
        };
      }
    case "attach":
      if (!rawArgs) {
        throw new Error("usage: /attach <path>");
      }
      actions.addAttachment(rawArgs);
      return {
        handled: true,
        entry: entry("Attachments", `Queued attachment:\n${rawArgs}`, "good")
      };
    case "attachments":
      return {
        handled: true,
        entry: entry(
          "Attachments",
          state.attachments.length
            ? state.attachments.map((attachment) => attachment.path).join("\n")
            : "attachments=(none)"
        )
      };
    case "detach":
    case "attachments-clear":
      actions.clearAttachments();
      return {
        handled: true,
        entry: entry("Attachments", "attachments cleared", "good")
      };
    case "new":
    case "clear":
      actions.startNewSession();
      return {
        handled: true,
        entry: entry("Session", "Started a new chat session.", "good")
      };
    case "diff": {
      const response = await workspaceDiff({ cwd: state.cwd || null });
      return {
        handled: true,
        entry: entry("Git diff", response.diff || "No diff output.")
      };
    }
    case "init": {
      const response = await workspaceInit({ cwd: state.cwd || null });
      return {
        handled: true,
        entry: entry(
          "Init",
          response.created ? `Initialized ${response.path}` : `${response.path} already exists.`,
          "good"
        )
      };
    }
    default:
      throw new Error(`Unknown slash command "${line}".`);
  }
}
