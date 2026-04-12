import type { ToolInvocation } from "./connectors";
import type { MemoryRecord, SessionSearchHit } from "./operations";
import type { TaskMode } from "./primitives";

export interface SessionSummary {
  id: string;
  title?: string | null;
  alias: string;
  provider_id: string;
  model: string;
  cwd?: string | null;
  task_mode?: TaskMode | null;
  created_at: string;
  updated_at: string;
}

export interface SessionMessage {
  id: string;
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  created_at: string;
}

export interface SessionTranscript {
  session: SessionSummary;
  messages: SessionMessage[];
}

export interface SessionResumePacket {
  session: SessionSummary;
  generated_at: string;
  recent_messages: SessionMessage[];
  linked_memories: MemoryRecord[];
  related_transcript_hits: SessionSearchHit[];
}

export interface RunTaskResponse {
  session_id: string;
  alias: string;
  provider_id: string;
  model: string;
  response: string;
  tool_events?: ToolInvocation[];
  structured_output_json?: string | null;
}
