import type { ReactNode } from "react";
import { Badge } from "./Badge";

export function MetricCard(props: {
  label: string;
  value: ReactNode;
  tone?: "neutral" | "info" | "good" | "warn" | "danger";
  detail?: string;
}) {
  return (
    <article className="metric-card">
      <div className="metric-card__meta">
        <span>{props.label}</span>
        {props.tone ? <Badge tone={props.tone}>{props.tone}</Badge> : null}
      </div>
      <strong className="metric-card__value">{props.value}</strong>
      {props.detail ? <p className="metric-card__detail">{props.detail}</p> : null}
    </article>
  );
}
