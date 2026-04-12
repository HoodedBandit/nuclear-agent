import { useEffect, useState, type ReactNode } from "react";
import clsx from "clsx";

export function DisclosureSection(props: {
  title: string;
  subtitle?: ReactNode;
  meta?: ReactNode;
  className?: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(Boolean(props.defaultOpen));

  useEffect(() => {
    if (props.defaultOpen) {
      setOpen(true);
    }
  }, [props.defaultOpen]);

  return (
    <details
      className={clsx("disclosure", props.className)}
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="disclosure__summary">
        <div className="disclosure__header">
          <strong>{props.title}</strong>
          {props.subtitle ? <span>{props.subtitle}</span> : null}
        </div>
        {props.meta ? <span className="disclosure__meta">{props.meta}</span> : null}
      </summary>
      <div className="disclosure__body">{props.children}</div>
    </details>
  );
}
