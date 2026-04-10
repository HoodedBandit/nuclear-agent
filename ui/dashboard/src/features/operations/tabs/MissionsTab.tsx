import type { FormEvent } from "react";
import type { Mission } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";
import { fmtDate } from "../format";

interface MissionsTabProps {
  missions?: Mission[];
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onUpdateMission: (missionId: string, action: "pause" | "resume" | "cancel") => void;
}

export function MissionsTab(props: MissionsTabProps) {
  const { missions, onSubmit, onUpdateMission } = props;

  return (
    <div className="split-panels">
      <Panel eyebrow="Schedule" title="Add mission">
        <form className="stack-list" onSubmit={onSubmit}>
          <label className="field">
            <span>Title</span>
            <input name="title" required />
          </label>
          <label className="field">
            <span>Details</span>
            <textarea name="details" required />
          </label>
          <div className="grid-three">
            <label className="field">
              <span>Alias</span>
              <input name="alias" />
            </label>
            <label className="field">
              <span>Model override</span>
              <input name="model" />
            </label>
            <label className="field">
              <span>Watch path</span>
              <input name="watch_path" />
            </label>
          </div>
          <div className="grid-three">
            <label className="field">
              <span>Wake after seconds</span>
              <input type="number" min="0" name="after_seconds" defaultValue="0" />
            </label>
            <label className="field">
              <span>Repeat every seconds</span>
              <input type="number" min="0" name="every_seconds" defaultValue="0" />
            </label>
          </div>
          <div className="button-row">
            <button type="submit">Queue mission</button>
          </div>
        </form>
      </Panel>

      <Panel eyebrow="Active" title="Mission ledger">
        <div className="stack-list">
          {missions?.length ? (
            missions.map((mission) => (
              <article key={mission.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{mission.title}</strong>
                  <span>{mission.status}</span>
                </div>
                <p className="stack-card__subtitle">{mission.details}</p>
                <p className="stack-card__copy">
                  wake {fmtDate(mission.wake_at)} | repeat {mission.repeat_interval_seconds || 0}s
                </p>
                <div className="button-row">
                  <button type="button" onClick={() => onUpdateMission(mission.id, "pause")}>
                    Pause
                  </button>
                  <button type="button" onClick={() => onUpdateMission(mission.id, "resume")}>
                    Resume
                  </button>
                  <button type="button" onClick={() => onUpdateMission(mission.id, "cancel")}>
                    Cancel
                  </button>
                </div>
              </article>
            ))
          ) : (
            <EmptyState title="No missions" copy="Queued or watched missions appear here." />
          )}
        </div>
      </Panel>
    </div>
  );
}
