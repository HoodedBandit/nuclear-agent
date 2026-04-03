import { useDashboardData } from "../../app/useDashboardData";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { fmtDate } from "../../utils/format";
import styles from "./SystemPage.module.css";

export function SystemPage() {
  const { bootstrap } = useDashboardData();

  return (
    <div className={styles.page} data-testid="modern-system-page">
      <div className={styles.grid}>
        <Surface eyebrow="Daemon" title="Runtime status" emphasis="accent">
          <div className={styles.metaList}>
            <div><span>PID</span><strong>{bootstrap.status.pid}</strong></div>
            <div><span>Started</span><strong>{fmtDate(bootstrap.status.started_at)}</strong></div>
            <div><span>Persistence</span><strong>{bootstrap.status.persistence_mode}</strong></div>
            <div><span>Auto start</span><strong>{bootstrap.status.auto_start ? "Enabled" : "Disabled"}</strong></div>
          </div>
        </Surface>

        <Surface eyebrow="Trust" title="Execution boundaries">
          <div className={styles.pillRow}>
            <Pill tone={bootstrap.trust.allow_shell ? "good" : "warn"}>Shell</Pill>
            <Pill tone={bootstrap.trust.allow_network ? "good" : "warn"}>Network</Pill>
            <Pill tone={bootstrap.trust.allow_full_disk ? "good" : "warn"}>Full disk</Pill>
            <Pill tone={bootstrap.trust.allow_self_edit ? "warn" : "neutral"}>Self-edit</Pill>
          </div>
          <div className={styles.trustedPaths}>
            {bootstrap.trust.trusted_paths.length > 0 ? (
              bootstrap.trust.trusted_paths.map((path) => (
                <code key={path} className={styles.pathChip}>{path}</code>
              ))
            ) : (
              <p className={styles.copy}>No trusted paths recorded.</p>
            )}
          </div>
        </Surface>
      </div>

      <Surface eyebrow="Capability map" title="Provider tool coverage">
        <div className={styles.capabilityList}>
          {bootstrap.provider_capabilities.map((item) => (
            <article key={`${item.provider_id}-${item.model}`} className={styles.capabilityCard}>
              <div>
                <strong>{item.provider_id}</strong>
                <div className={styles.copy}>{item.model}</div>
              </div>
              <div className={styles.capabilities}>
                {Object.entries(item.capabilities)
                  .filter(([, enabled]) => enabled)
                  .map(([capability]) => (
                    <Pill key={capability} tone="accent">{capability.replace(/_/g, " ")}</Pill>
                  ))}
              </div>
            </article>
          ))}
        </div>
      </Surface>

      <Surface eyebrow="Classic bridge" title="Advanced admin">
        <p className={styles.copy}>
          Advanced JSON config editing, full connector management, and legacy operator-only controls
          remain available in the classic dashboard during the staged rollout.
        </p>
        <a className={styles.linkButton} href="/dashboard-classic">Open classic dashboard</a>
      </Surface>
    </div>
  );
}
