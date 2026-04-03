import type { PropsWithChildren, ReactNode } from "react";
import clsx from "clsx";

import styles from "./Surface.module.css";

interface SurfaceProps extends PropsWithChildren {
  title?: string;
  eyebrow?: string;
  actions?: ReactNode;
  emphasis?: "default" | "accent";
  className?: string;
}

export function Surface({
  title,
  eyebrow,
  actions,
  emphasis = "default",
  className,
  children
}: SurfaceProps) {
  return (
    <section
      className={clsx(styles.surface, emphasis === "accent" && styles.accent, className)}
    >
      {(title || eyebrow || actions) && (
        <header className={styles.header}>
          <div>
            {eyebrow ? <div className={styles.eyebrow}>{eyebrow}</div> : null}
            {title ? <h2 className={styles.title}>{title}</h2> : null}
          </div>
          {actions ? <div className={styles.actions}>{actions}</div> : null}
        </header>
      )}
      {children}
    </section>
  );
}
