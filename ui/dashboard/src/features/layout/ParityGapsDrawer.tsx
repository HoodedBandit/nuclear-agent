import clsx from "clsx";

type GapStatus = "wired" | "adapted" | "unsupported";

interface GapEntry {
  surface: string;
  feature: string;
  status: GapStatus;
  note: string;
}

const GAP_ENTRIES: GapEntry[] = [
  {
    surface: "Channels",
    feature: "WhatsApp, Google Chat, iMessage, Nostr",
    status: "unsupported",
    note: "Not exposed by Nuclear today."
  },
  {
    surface: "Overview",
    feature: "Gateway pairing and trusted-proxy hints",
    status: "unsupported",
    note: "Nuclear uses dashboard token auth."
  },
  {
    surface: "Sessions",
    feature: "Agent file and identity editors",
    status: "unsupported",
    note: "Backend does not expose agent file editing."
  },
  {
    surface: "Automation",
    feature: "OpenClaw cron manager",
    status: "unsupported",
    note: "Nuclear missions cover scheduling instead."
  },
  {
    surface: "Agent",
    feature: "Dreaming flows",
    status: "unsupported",
    note: "No matching Nuclear runtime surface."
  },
  {
    surface: "Control",
    feature: "Instances and nodes",
    status: "unsupported",
    note: "No node inventory API in Nuclear."
  },
  {
    surface: "Infrastructure",
    feature: "Providers and aliases",
    status: "adapted",
    note: "Mapped onto Nuclear provider and alias APIs."
  },
  {
    surface: "Config",
    feature: "Settings shell and update controls",
    status: "adapted",
    note: "Rendered with Nuclear config, trust, and update flows."
  },
  {
    surface: "Chat",
    feature: "Session run flow",
    status: "wired",
    note: "Backed by Nuclear run and session APIs."
  },
  {
    surface: "Logs",
    feature: "Event and daemon log feed",
    status: "wired",
    note: "Backed by Nuclear log and event APIs."
  }
];

const STATUS_ORDER: GapStatus[] = ["wired", "adapted", "unsupported"];

export function ParityGapsDrawer(props: {
  open: boolean;
  onClose: () => void;
}) {
  const groupedEntries = STATUS_ORDER.map((status) => ({
    status,
    entries: GAP_ENTRIES.filter((entry) => entry.status === status)
  }));

  return (
    <>
      <button
        type="button"
        className={clsx("drawer-backdrop", props.open && "drawer-backdrop--open")}
        aria-label="Close parity gaps"
        onClick={props.onClose}
      />
      <aside
        className={clsx("gap-drawer", props.open && "gap-drawer--open")}
        aria-hidden={!props.open}
      >
        <div className="gap-drawer__header">
          <div>
            <p className="section-label">Parity gaps</p>
            <h2>OpenClaw delta</h2>
          </div>
          <button type="button" className="icon-button" onClick={props.onClose}>
            Close
          </button>
        </div>
        <div className="gap-drawer__body">
          {groupedEntries.map((group) => (
            <section key={group.status} className="gap-section">
              <div className="gap-section__header">
                <h3>{group.status}</h3>
                <span>{group.entries.length}</span>
              </div>
              <div className="gap-list">
                {group.entries.map((entry) => (
                  <article key={`${entry.surface}-${entry.feature}`} className="gap-card">
                    <div className="gap-card__title">
                      <strong>{entry.feature}</strong>
                      <span>{entry.surface}</span>
                    </div>
                    <p>{entry.note}</p>
                  </article>
                ))}
              </div>
            </section>
          ))}
        </div>
      </aside>
    </>
  );
}
