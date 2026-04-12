import type {
  MemoryKind,
  MemoryReviewStatus,
  MemoryScope,
  MissionStatus
} from "./primitives";

export interface MemoryRecord {
  id: string;
  kind: MemoryKind;
  scope: MemoryScope;
  subject: string;
  content: string;
  confidence: number;
  review_status: MemoryReviewStatus;
  tags?: string[];
  workspace_key?: string | null;
  provider_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface SessionSearchHit {
  session_id: string;
  score: number;
  title?: string | null;
  snippet: string;
}

export interface MemorySearchResponse {
  memories: MemoryRecord[];
  transcript_hits: SessionSearchHit[];
}

export interface MemoryRebuildResponse {
  generated_at: string;
  sessions_scanned: number;
  observations_scanned: number;
  memories_upserted: number;
  embeddings_refreshed: number;
}

export interface Mission {
  id: string;
  title: string;
  details: string;
  status: MissionStatus;
  created_at: string;
  updated_at: string;
  alias?: string | null;
  requested_model?: string | null;
  session_id?: string | null;
  phase?: string | null;
  handoff_summary?: string | null;
  workspace_key?: string | null;
  watch_path?: string | null;
  watch_recursive?: boolean;
  wake_at?: string | null;
  repeat_interval_seconds?: number | null;
  wake_trigger?: string | null;
  retries?: number;
  max_retries?: number;
}

export interface MissionCheckpoint {
  mission_id: string;
  status: MissionStatus;
  summary: string;
  created_at: string;
  session_id?: string | null;
}

export interface SkillDraft {
  id: string;
  title: string;
  content: string;
  status: "draft" | "published" | "rejected";
  created_at: string;
  updated_at: string;
}
