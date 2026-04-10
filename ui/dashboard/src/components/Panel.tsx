import type { ReactNode } from "react";
import clsx from "clsx";

export function Panel(props: {
  eyebrow?: string;
  title: string;
  meta?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={clsx("panel-surface", props.className)}>
      <header className="panel-surface__header">
        <div>
          {props.eyebrow ? <p className="eyebrow">{props.eyebrow}</p> : null}
          <h2>{props.title}</h2>
        </div>
        {props.meta ? <div className="panel-surface__meta">{props.meta}</div> : null}
      </header>
      <div className="panel-surface__body">{props.children}</div>
    </section>
  );
}
