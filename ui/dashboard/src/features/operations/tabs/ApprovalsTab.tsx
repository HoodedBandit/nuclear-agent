import type { ConnectorApprovalRecord } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";

interface ApprovalsTabProps {
  approvals?: ConnectorApprovalRecord[];
  onDecide: (approvalId: string, action: "approve" | "reject") => void;
}

export function ApprovalsTab(props: ApprovalsTabProps) {
  const { approvals, onDecide } = props;

  return (
    <div className="stack-list">
      {approvals?.length ? (
        approvals.map((approval) => (
          <article key={approval.id} className="stack-card">
            <div className="stack-card__title">
              <strong>{approval.title}</strong>
              <span>{approval.status}</span>
            </div>
            <p className="stack-card__subtitle">
              {approval.connector_kind} / {approval.connector_name}
            </p>
            <p className="stack-card__copy">{approval.details}</p>
            <div className="button-row">
              <button type="button" onClick={() => onDecide(approval.id, "approve")}>
                Approve
              </button>
              <button type="button" onClick={() => onDecide(approval.id, "reject")}>
                Reject
              </button>
            </div>
          </article>
        ))
      ) : (
        <EmptyState title="No pending approvals" copy="Connector approvals surface here." />
      )}
    </div>
  );
}
