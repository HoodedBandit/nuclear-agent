import type {
  InputAttachment,
  PermissionPreset,
  RemoteContentPolicy,
  TaskMode,
  ThinkingLevel
} from "../../api/types";

export const THINKING_LEVELS: ThinkingLevel[] = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh"
];

export const TASK_MODES: Array<TaskMode | ""> = ["", "build", "daily"];

export const PERMISSION_PRESETS: Array<PermissionPreset | ""> = [
  "",
  "suggest",
  "auto_edit",
  "full_auto"
];

export const REMOTE_POLICIES: Array<RemoteContentPolicy | ""> = [
  "",
  "allow",
  "warn_only",
  "block_high_risk"
];

export const ATTACHMENT_KINDS: InputAttachment["kind"][] = ["file", "image"];
