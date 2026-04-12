import type { FormEvent } from "react";
import { useState } from "react";
import type { MemoryRecord, MemorySearchResponse } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";

interface MemoryTabProps {
  reviewMemories?: MemoryRecord[];
  memorySearchResults: MemorySearchResponse | null;
  onDecideMemory: (memoryId: string, action: "approve" | "reject") => void;
  onSearch: (event: FormEvent<HTMLFormElement>) => void;
  onCreate: (event: FormEvent<HTMLFormElement>) => void;
  onRebuild: (event: FormEvent<HTMLFormElement>) => void;
  onForget: (memoryId: string) => void;
}

export function MemoryTab(props: MemoryTabProps) {
  const {
    reviewMemories,
    memorySearchResults,
    onDecideMemory,
    onSearch,
    onCreate,
    onRebuild,
    onForget
  } = props;
  const [activeTool, setActiveTool] = useState<"search" | "create" | "rebuild">("search");

  return (
    <div className="split-panels">
      <Panel eyebrow="Review" title="Memory queue">
        <div className="stack-list">
          {reviewMemories?.length ? (
            reviewMemories.map((memory) => (
              <article key={memory.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{memory.subject}</strong>
                  <span>{memory.kind}</span>
                </div>
                <p className="stack-card__copy">{memory.content}</p>
                <div className="button-row">
                  <button type="button" onClick={() => onDecideMemory(memory.id, "approve")}>
                    Approve
                  </button>
                  <button type="button" onClick={() => onDecideMemory(memory.id, "reject")}>
                    Reject
                  </button>
                </div>
              </article>
            ))
          ) : (
            <EmptyState title="Queue clear" copy="No candidate memories need review." />
          )}
        </div>
      </Panel>

      <Panel eyebrow="Manual" title="Memory tools">
        <div className="stack-list">
          <div className="subtabs subtabs--panel" role="tablist" aria-label="Memory tools">
            {(["search", "create", "rebuild"] as const).map((tool) => (
              <button
                key={tool}
                type="button"
                className={activeTool === tool ? "is-active" : undefined}
                onClick={() => setActiveTool(tool)}
              >
                {tool}
              </button>
            ))}
          </div>

          {activeTool === "search" ? (
            <>
              <form className="stack-card stack-list" onSubmit={onSearch}>
                <label className="field">
                  <span>Search memory</span>
                  <input name="query" placeholder="preferred workflow" />
                </label>
                <button type="submit">Search</button>
              </form>
              {memorySearchResults?.memories.length ? (
                <div className="stack-list">
                  {memorySearchResults.memories.map((memory) => (
                    <article key={memory.id} className="stack-card">
                      <div className="stack-card__title">
                        <strong>{memory.subject}</strong>
                        <span>{memory.review_status}</span>
                      </div>
                      <p className="stack-card__copy">{memory.content}</p>
                      <div className="button-row">
                        <button type="button" onClick={() => onForget(memory.id)}>
                          Forget
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <EmptyState
                  title="No results yet"
                  copy="Run a query to inspect retained memory before making changes."
                />
              )}
            </>
          ) : null}

          {activeTool === "create" ? (
            <form className="stack-card stack-list" onSubmit={onCreate}>
              <div className="grid-three">
                <label className="field">
                  <span>Kind</span>
                  <select name="kind" defaultValue="note">
                    <option value="preference">preference</option>
                    <option value="project_fact">project_fact</option>
                    <option value="workflow">workflow</option>
                    <option value="constraint">constraint</option>
                    <option value="task">task</option>
                    <option value="note">note</option>
                  </select>
                </label>
                <label className="field">
                  <span>Scope</span>
                  <select name="scope" defaultValue="workspace">
                    <option value="global">global</option>
                    <option value="workspace">workspace</option>
                    <option value="session">session</option>
                    <option value="provider">provider</option>
                  </select>
                </label>
                <label className="field">
                  <span>Subject</span>
                  <input name="subject" required />
                </label>
              </div>
              <label className="field">
                <span>Content</span>
                <textarea name="content" required />
              </label>
              <button type="submit">Create memory</button>
            </form>
          ) : null}

          {activeTool === "rebuild" ? (
            <form className="stack-card stack-list" onSubmit={onRebuild}>
              <label className="field">
                <span>Session ID (optional)</span>
                <input name="session_id" />
              </label>
              <label className="field">
                <span>
                  <input type="checkbox" name="recompute_embeddings" /> Recompute embeddings
                </span>
              </label>
              <button type="submit">Rebuild memory</button>
            </form>
          ) : null}
        </div>
      </Panel>
    </div>
  );
}
