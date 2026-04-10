import type { FormEvent } from "react";
import type { DaemonStatus } from "../../../api/types";
import { Panel } from "../../../components/Panel";

interface DaemonTabProps {
  status: DaemonStatus;
  onUpdateDaemon: (event: FormEvent<HTMLFormElement>) => void;
  onUpdateAutonomy: (mode: "enable" | "pause" | "resume") => void;
  onUpdateEvolve: (mode: "start" | "pause" | "resume" | "stop") => void;
  onUpdateAutopilot: (event: FormEvent<HTMLFormElement>) => void;
}

export function DaemonTab(props: DaemonTabProps) {
  const {
    status,
    onUpdateDaemon,
    onUpdateAutonomy,
    onUpdateEvolve,
    onUpdateAutopilot
  } = props;

  return (
    <div className="split-panels">
      <Panel eyebrow="Daemon" title="Persistence and startup">
        <form className="stack-list" id="daemon-config-form" onSubmit={onUpdateDaemon}>
          <label className="field">
            <span>Persistence mode</span>
            <select
              id="daemon-persistence-mode"
              name="persistence_mode"
              defaultValue={status.persistence_mode}
            >
              <option value="on_demand">on_demand</option>
              <option value="always_on">always_on</option>
            </select>
          </label>
          <label className="field">
            <span>
              <input type="checkbox" name="auto_start" defaultChecked={status.auto_start} /> Auto
              start
            </span>
          </label>
          <button type="submit">Update daemon config</button>
        </form>
      </Panel>

      <Panel eyebrow="Autonomy" title="Runtime posture">
        <div className="stack-list">
          <article className="stack-card">
            <div className="stack-card__title">
              <strong>Autonomy</strong>
              <span>{status.autonomy.state}</span>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => onUpdateAutonomy("enable")}>
                Enable
              </button>
              <button type="button" onClick={() => onUpdateAutonomy("pause")}>
                Pause
              </button>
              <button type="button" onClick={() => onUpdateAutonomy("resume")}>
                Resume
              </button>
            </div>
          </article>
          <article className="stack-card">
            <div className="stack-card__title">
              <strong>Evolve</strong>
              <span>{status.evolve.state}</span>
            </div>
            <div className="button-row">
              <button type="button" onClick={() => onUpdateEvolve("start")}>
                Start
              </button>
              <button type="button" onClick={() => onUpdateEvolve("pause")}>
                Pause
              </button>
              <button type="button" onClick={() => onUpdateEvolve("resume")}>
                Resume
              </button>
              <button type="button" onClick={() => onUpdateEvolve("stop")}>
                Stop
              </button>
            </div>
          </article>
          <form className="stack-card stack-list" onSubmit={onUpdateAutopilot}>
            <div className="stack-card__title">
              <strong>Autopilot</strong>
              <span>{status.autopilot.state}</span>
            </div>
            <div className="grid-three">
              <label className="field">
                <span>State</span>
                <select
                  id="autopilot-state"
                  name="state"
                  defaultValue={status.autopilot.state}
                >
                  <option value="disabled">disabled</option>
                  <option value="enabled">enabled</option>
                  <option value="paused">paused</option>
                </select>
              </label>
              <label className="field">
                <span>Max concurrent missions</span>
                <input
                  type="number"
                  name="max_concurrent_missions"
                  defaultValue={status.autopilot.max_concurrent_missions}
                />
              </label>
              <label className="field">
                <span>Wake interval seconds</span>
                <input
                  type="number"
                  name="wake_interval_seconds"
                  defaultValue={status.autopilot.wake_interval_seconds}
                />
              </label>
            </div>
            <label className="field">
              <span>
                <input
                  type="checkbox"
                  name="allow_background_shell"
                  defaultChecked={status.autopilot.allow_background_shell}
                />{" "}
                Allow background shell
              </span>
            </label>
            <label className="field">
              <span>
                <input
                  type="checkbox"
                  name="allow_background_network"
                  defaultChecked={status.autopilot.allow_background_network}
                />{" "}
                Allow background network
              </span>
            </label>
            <label className="field">
              <span>
                <input
                  type="checkbox"
                  name="allow_background_self_edit"
                  defaultChecked={status.autopilot.allow_background_self_edit}
                />{" "}
                Allow background self edit
              </span>
            </label>
            <button type="submit">Update autopilot</button>
          </form>
        </div>
      </Panel>
    </div>
  );
}
