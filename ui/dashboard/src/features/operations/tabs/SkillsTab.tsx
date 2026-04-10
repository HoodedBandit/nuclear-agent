import type { FormEvent } from "react";
import type { MemoryRecord, SkillDraft } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";

interface SkillsTabProps {
  enabledSkills?: string[];
  profileMemories?: MemoryRecord[];
  skillDrafts?: SkillDraft[];
  onUpdateSkills: (event: FormEvent<HTMLFormElement>) => void;
  onPublishDraft: (draftId: string, action: "publish" | "reject") => void;
}

export function SkillsTab(props: SkillsTabProps) {
  const {
    enabledSkills,
    profileMemories,
    skillDrafts,
    onUpdateSkills,
    onPublishDraft
  } = props;

  return (
    <div className="split-panels">
      <Panel eyebrow="Enabled" title="Skill set">
        <form className="stack-list" onSubmit={onUpdateSkills}>
          <label className="field">
            <span>Enabled skills (comma separated)</span>
            <input
              name="skills"
              defaultValue={(enabledSkills || []).join(", ")}
              placeholder="imagegen, github"
            />
          </label>
          <button type="submit">Update enabled skills</button>
        </form>
        <div className="stack-list">
          {(profileMemories || []).slice(0, 4).map((memory) => (
            <article key={memory.id} className="stack-card">
              <div className="stack-card__title">
                <strong>{memory.subject}</strong>
                <span>{memory.kind}</span>
              </div>
              <p className="stack-card__copy">{memory.content}</p>
            </article>
          ))}
        </div>
      </Panel>

      <Panel eyebrow="Drafts" title="Skill review">
        <div className="stack-list">
          {skillDrafts?.length ? (
            skillDrafts.map((draft) => (
              <article key={draft.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{draft.title}</strong>
                  <span>{draft.status}</span>
                </div>
                <p className="stack-card__copy">{draft.content}</p>
                <div className="button-row">
                  <button type="button" onClick={() => onPublishDraft(draft.id, "publish")}>
                    Publish
                  </button>
                  <button type="button" onClick={() => onPublishDraft(draft.id, "reject")}>
                    Reject
                  </button>
                </div>
              </article>
            ))
          ) : (
            <EmptyState title="No drafts" copy="Learned skill drafts appear here." />
          )}
        </div>
      </Panel>
    </div>
  );
}
