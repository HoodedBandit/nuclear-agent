import type { ReactNode } from "react";
import clsx from "clsx";

export function Badge(props: {
  tone?: "neutral" | "info" | "good" | "warn" | "danger";
  children: ReactNode;
}) {
  return (
    <span className={clsx("badge", props.tone && `badge--${props.tone}`)}>
      {props.children}
    </span>
  );
}
