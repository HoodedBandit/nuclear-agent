import type { FormEvent } from "react";
import type {
  InputAttachment,
  ModelAlias,
  PermissionPreset,
  RemoteContentPolicy,
  TaskMode,
  ThinkingLevel
} from "../../../api/types";
import { DisclosureSection } from "../../../components/DisclosureSection";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";
import {
  ATTACHMENT_KINDS,
  PERMISSION_PRESETS,
  REMOTE_POLICIES,
  TASK_MODES,
  THINKING_LEVELS
} from "../constants";

interface RunTaskPanelProps {
  aliases: ModelAlias[];
  sessionId: string | null;
  prompt: string;
  alias: string;
  thinking: ThinkingLevel;
  cwd: string;
  taskMode: TaskMode | "";
  permissionPreset: PermissionPreset | "";
  ephemeral: boolean;
  remoteContentPolicy: RemoteContentPolicy | "";
  attachmentPath: string;
  attachmentKind: InputAttachment["kind"];
  attachments: InputAttachment[];
  busy: boolean;
  error: string | null;
  onPromptChange: (value: string) => void;
  onAliasChange: (value: string) => void;
  onThinkingChange: (value: ThinkingLevel) => void;
  onCwdChange: (value: string) => void;
  onTaskModeChange: (value: TaskMode | "") => void;
  onPermissionPresetChange: (value: PermissionPreset | "") => void;
  onEphemeralChange: (value: boolean) => void;
  onRemoteContentPolicyChange: (value: RemoteContentPolicy | "") => void;
  onAttachmentPathChange: (value: string) => void;
  onAttachmentKindChange: (value: InputAttachment["kind"]) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onAddAttachment: () => void;
  onRemoveAttachment: (index: number) => void;
  onMakeMain: () => void;
  onClearSession: () => void;
  onRenameSession: () => void;
  onForkSession: () => void;
  onCompactSession: () => void;
}

export function RunTaskPanel({
  aliases,
  sessionId,
  prompt,
  alias,
  thinking,
  cwd,
  taskMode,
  permissionPreset,
  ephemeral,
  remoteContentPolicy,
  attachmentPath,
  attachmentKind,
  attachments,
  busy,
  error,
  onPromptChange,
  onAliasChange,
  onThinkingChange,
  onCwdChange,
  onTaskModeChange,
  onPermissionPresetChange,
  onEphemeralChange,
  onRemoteContentPolicyChange,
  onAttachmentPathChange,
  onAttachmentKindChange,
  onSubmit,
  onAddAttachment,
  onRemoveAttachment,
  onMakeMain,
  onClearSession,
  onRenameSession,
  onForkSession,
  onCompactSession
}: RunTaskPanelProps) {
  const overridesOpen = Boolean(
    cwd.trim() || taskMode || permissionPreset || remoteContentPolicy || ephemeral
  );
  const attachmentsOpen = Boolean(attachments.length || attachmentPath.trim());

  return (
    <Panel eyebrow="Chat" title="Run task" meta={sessionId || "new session"}>
      <div className="stack-card stack-card--summary" id="chat-session-meta">
        <div className="stack-card__title">
          <strong>Session target</strong>
          <span className="mono">{sessionId || "draft"}</span>
        </div>
        <div className="fact-grid">
          <article className="fact-card">
            <span>Alias</span>
            <strong>{alias}</strong>
          </article>
          <article className="fact-card">
            <span>Mode</span>
            <strong>{taskMode || "default"}</strong>
          </article>
          <article className="fact-card">
            <span>Working dir</span>
            <strong>{cwd || "daemon cwd"}</strong>
          </article>
          <article className="fact-card">
            <span>Attachments</span>
            <strong>{attachments.length}</strong>
          </article>
        </div>
      </div>
      <form className="stack-list" id="run-task-form" onSubmit={onSubmit}>
        <label className="field">
          <span>Prompt</span>
          <textarea
            id="run-task-prompt"
            value={prompt}
            onChange={(event) => onPromptChange(event.target.value)}
          />
        </label>
        <div className="grid-two">
          <label className="field">
            <span>Alias</span>
            <select
              id="run-task-alias"
              value={alias}
              onChange={(event) => onAliasChange(event.target.value)}
            >
              {aliases.map((entry) => (
                <option key={entry.alias} value={entry.alias}>
                  {entry.alias}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Thinking</span>
            <select
              id="run-task-thinking"
              value={thinking}
              onChange={(event) => onThinkingChange(event.target.value as ThinkingLevel)}
            >
              {THINKING_LEVELS.map((entry) => (
                <option key={entry} value={entry}>
                  {entry}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="button-row">
          <button id="run-task-submit" type="submit" disabled={busy || !prompt.trim()}>
            {busy ? "Running..." : "Run task"}
          </button>
          <button id="chat-new-session" type="button" onClick={onClearSession}>
            New session
          </button>
          <button id="chat-make-main-button" type="button" onClick={onMakeMain}>
            Make main
          </button>
        </div>
        <DisclosureSection
          title="Runtime overrides"
          subtitle="Working directory, task mode, permissions, and remote access"
          meta={overridesOpen ? "configured" : "inherit defaults"}
          defaultOpen={overridesOpen}
        >
          <div className="stack-list">
            <div className="grid-three">
              <label className="field">
                <span>Working directory</span>
                <input
                  id="run-task-cwd"
                  value={cwd}
                  onChange={(event) => onCwdChange(event.target.value)}
                />
              </label>
              <label className="field">
                <span>Task mode</span>
                <select
                  id="run-task-mode"
                  value={taskMode}
                  onChange={(event) => onTaskModeChange(event.target.value as TaskMode | "")}
                >
                  {TASK_MODES.map((entry) => (
                    <option key={entry || "default"} value={entry}>
                      {entry || "default"}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>Permission preset</span>
                <select
                  id="run-task-permissions"
                  value={permissionPreset}
                  onChange={(event) =>
                    onPermissionPresetChange(event.target.value as PermissionPreset | "")
                  }
                >
                  {PERMISSION_PRESETS.map((entry) => (
                    <option key={entry || "inherit"} value={entry}>
                      {entry || "inherit"}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div className="grid-two">
              <label className="field">
                <span>Remote content policy</span>
                <select
                  id="run-task-remote-policy"
                  value={remoteContentPolicy}
                  onChange={(event) =>
                    onRemoteContentPolicyChange(
                      event.target.value as RemoteContentPolicy | ""
                    )
                  }
                >
                  {REMOTE_POLICIES.map((entry) => (
                    <option key={entry || "default"} value={entry}>
                      {entry || "default"}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>
                  <input
                    id="run-task-ephemeral"
                    type="checkbox"
                    checked={ephemeral}
                    onChange={(event) => onEphemeralChange(event.target.checked)}
                  />{" "}
                  Ephemeral run
                </span>
              </label>
            </div>
          </div>
        </DisclosureSection>
        <DisclosureSection
          title="Attachments"
          subtitle="Stage files and images into the next run"
          meta={`${attachments.length} staged`}
          defaultOpen={attachmentsOpen}
        >
          <div className="stack-list" id="chat-attachments-panel">
            <div className="grid-three">
              <label className="field">
                <span>Attachment kind</span>
                <select
                  id="chat-attachment-kind"
                  value={attachmentKind}
                  onChange={(event) =>
                    onAttachmentKindChange(event.target.value as InputAttachment["kind"])
                  }
                >
                  {ATTACHMENT_KINDS.map((entry) => (
                    <option key={entry} value={entry}>
                      {entry}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>Attachment path</span>
                <input
                  id="chat-attachment-path"
                  value={attachmentPath}
                  onChange={(event) => onAttachmentPathChange(event.target.value)}
                  placeholder="J:\\assets\\reference.png"
                />
              </label>
              <div className="field">
                <span>Stage attachment</span>
                <button
                  id="chat-attachment-add"
                  type="button"
                  onClick={onAddAttachment}
                  disabled={!attachmentPath.trim()}
                >
                  Add attachment
                </button>
              </div>
            </div>
            <div className="stack-list" id="chat-attachments">
              {attachments.length ? (
                attachments.map((attachment, index) => (
                  <article
                    key={`${attachment.kind}-${attachment.path}-${index}`}
                    className="stack-card"
                  >
                    <div className="stack-card__title">
                      <strong>{attachment.kind}</strong>
                      <span className="mono">{attachment.path}</span>
                    </div>
                    <div className="button-row">
                      <button type="button" onClick={() => onRemoveAttachment(index)}>
                        Remove
                      </button>
                    </div>
                  </article>
                ))
              ) : (
                <EmptyState
                  title="No attachments"
                  copy="Add image or file paths to carry context into the task."
                />
              )}
            </div>
          </div>
        </DisclosureSection>
        <DisclosureSection
          title="Session controls"
          subtitle="Rename, fork, or compact the current thread"
          meta={sessionId ? "live session" : "draft only"}
          defaultOpen={Boolean(sessionId)}
        >
          <div className="button-row">
            <button type="button" onClick={onRenameSession}>
              Rename
            </button>
            <button type="button" onClick={onForkSession}>
              Fork
            </button>
            <button type="button" onClick={onCompactSession}>
              Compact
            </button>
          </div>
        </DisclosureSection>
        {error ? <p className="error-copy">{error}</p> : null}
      </form>
    </Panel>
  );
}
