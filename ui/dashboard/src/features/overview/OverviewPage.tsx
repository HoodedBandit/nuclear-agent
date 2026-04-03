import { useDashboardData } from "../../app/useDashboardData";
import { Pill } from "../../components/Pill";
import { StatCard } from "../../components/StatCard";
import { Surface } from "../../components/Surface";
import { fmtCount, fmtDate, startCase } from "../../utils/format";
import styles from "./OverviewPage.module.css";

export function OverviewPage() {
  const { bootstrap } = useDashboardData();
  const connectorCount =
    bootstrap.telegram_connectors.length +
    bootstrap.discord_connectors.length +
    bootstrap.slack_connectors.length +
    bootstrap.signal_connectors.length +
    bootstrap.home_assistant_connectors.length +
    bootstrap.webhook_connectors.length +
    bootstrap.inbox_connectors.length +
    bootstrap.gmail_connectors.length +
    bootstrap.brave_connectors.length;

  return (
    <div className={styles.page} data-testid="modern-overview-page">
      <section className={styles.hero}>
        <div>
          <div className={styles.heroEyebrow}>Operator overview</div>
          <h2 className={styles.heroTitle}>Control-room summary for the daemon, targets, and recent activity.</h2>
        </div>
        <div className={styles.heroChips}>
          <Pill tone="accent">{startCase(bootstrap.status.persistence_mode)}</Pill>
          <Pill tone={bootstrap.status.autonomy.allow_self_edit ? "warn" : "neutral"}>
            Self-edit {bootstrap.status.autonomy.allow_self_edit ? "enabled" : "off"}
          </Pill>
          <Pill tone="good">{startCase(bootstrap.remote_content_policy)}</Pill>
        </div>
      </section>

      <section className={styles.statsGrid}>
        <StatCard
          label="Providers"
          value={fmtCount(bootstrap.providers.length)}
          detail={`${bootstrap.provider_capabilities.length} advertised model capability summaries`}
        />
        <StatCard
          label="Aliases"
          value={fmtCount(bootstrap.aliases.length)}
          detail={`Main alias ${bootstrap.status.main_agent_alias ?? "is not configured"}`}
        />
        <StatCard
          label="Connectors"
          value={fmtCount(connectorCount)}
          detail={`${fmtCount(bootstrap.plugins.length)} installed plugins tracked`}
        />
        <StatCard
          label="Sessions"
          value={fmtCount(bootstrap.sessions.length)}
          detail={`Daemon started ${fmtDate(bootstrap.status.started_at)}`}
        />
      </section>

      <div className={styles.grid}>
        <Surface eyebrow="Targeting" title="Main target" emphasis="accent">
          <div className={styles.targetCard}>
            <div>
              <span className={styles.label}>Alias</span>
              <strong>{bootstrap.status.main_target?.alias ?? bootstrap.status.main_agent_alias ?? "Unassigned"}</strong>
            </div>
            <div>
              <span className={styles.label}>Provider</span>
              <strong>{bootstrap.status.main_target?.provider_id ?? "Unavailable"}</strong>
            </div>
            <div>
              <span className={styles.label}>Model</span>
              <strong>{bootstrap.status.main_target?.model ?? "Unavailable"}</strong>
            </div>
          </div>
        </Surface>

        <Surface eyebrow="Policy" title="Execution posture">
          <div className={styles.policyList}>
            <div><span>Permission preset</span><strong>{startCase(bootstrap.permissions)}</strong></div>
            <div><span>Network</span><strong>{bootstrap.trust.allow_network ? "Allowed" : "Guarded"}</strong></div>
            <div><span>Shell</span><strong>{bootstrap.trust.allow_shell ? "Allowed" : "Guarded"}</strong></div>
            <div><span>Full disk</span><strong>{bootstrap.trust.allow_full_disk ? "Allowed" : "Guarded"}</strong></div>
          </div>
        </Surface>

        <Surface eyebrow="Provider grid" title="Configured providers">
          <div className={styles.list}>
            {bootstrap.providers.map((provider) => (
              <article key={provider.id} className={styles.listCard}>
                <div>
                  <strong>{provider.display_name}</strong>
                  <div className={styles.meta}>{provider.id}</div>
                </div>
                <div className={styles.meta}>{startCase(provider.kind)}</div>
              </article>
            ))}
          </div>
        </Surface>

        <Surface eyebrow="Recent sessions" title="Active conversation history">
          <div className={styles.list}>
            {bootstrap.sessions.slice(0, 6).map((session) => (
              <article key={session.id} className={styles.listCard}>
                <div>
                  <strong>{session.title ?? "Untitled session"}</strong>
                  <div className={styles.meta}>{session.alias} · {session.model}</div>
                </div>
                <div className={styles.meta}>{fmtDate(session.updated_at)}</div>
              </article>
            ))}
          </div>
        </Surface>
      </div>
    </div>
  );
}
