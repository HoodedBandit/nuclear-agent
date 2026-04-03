import type { ReactNode } from "react";
import clsx from "clsx";

import styles from "./Pill.module.css";

interface PillProps {
  tone?: "neutral" | "good" | "warn" | "danger" | "accent";
  children: ReactNode;
}

export function Pill({ tone = "neutral", children }: PillProps) {
  return <span className={clsx(styles.pill, styles[tone])}>{children}</span>;
}
