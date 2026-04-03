import { useDashboardData } from "../../app/useDashboardData";
import { EmptyState } from "../../components/EmptyState";
import { Surface } from "../../components/Surface";
import styles from "./OperationsPage.module.css";

export function OperationsPage() {
  const { bootstrap } = useDashboardData();

  return (
    <div className={styles.page} data-testid="modern-operations-page">
      <Surface eyebrow="Operations" title="Missions, approvals, memory, and queue surfaces">
        <p className={styles.copy}>
          This staged cockpit keeps operational workflows intact while the dense modern workbenches
          are migrated out of the classic dashboard. The core daemon state is already available, and
          the full legacy operator console remains one click away.
        </p>
        <div className={styles.callouts}>
          <article className={styles.callout}>
            <strong>Recent events</strong>
            <span>{bootstrap.events.length} events available in the current bootstrap payload.</span>
          </article>
          <article className={styles.callout}>
            <strong>Delegation targets</strong>
            <span>{bootstrap.delegation_targets.length} targets ready for sub-agent orchestration.</span>
          </article>
          <article className={styles.callout}>
            <strong>Plugins</strong>
            <span>{bootstrap.plugins.length} plugins currently tracked by the daemon.</span>
          </article>
        </div>
      </Surface>

      <Surface eyebrow="Classic bridge" title="Use the legacy queue tools while migration continues">
        <EmptyState
          title="Operations workbench still staged"
          body="Open the classic dashboard for full missions, approvals, memory review, plugin doctoring, and long-tail admin tooling until those surfaces are reimplemented here."
          action={<a className={styles.linkButton} href="/dashboard-classic">Open classic dashboard</a>}
        />
      </Surface>
    </div>
  );
}
