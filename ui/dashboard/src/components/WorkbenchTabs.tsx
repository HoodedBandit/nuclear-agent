import clsx from "clsx";

import styles from "./WorkbenchTabs.module.css";

export interface WorkbenchTab {
  id: string;
  label: string;
  description?: string;
}

interface WorkbenchTabsProps {
  tabs: WorkbenchTab[];
  activeTab: string;
  onChange: (tabId: string) => void;
  className?: string;
  testIdPrefix?: string;
}

export function WorkbenchTabs({
  tabs,
  activeTab,
  onChange,
  className,
  testIdPrefix
}: WorkbenchTabsProps) {
  return (
    <div className={clsx(styles.tabStrip, className)} role="tablist">
      {tabs.map((tab) => {
        const selected = tab.id === activeTab;
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={selected}
            className={clsx(styles.tab, selected && styles.tabActive)}
            onClick={() => onChange(tab.id)}
            data-testid={testIdPrefix ? `${testIdPrefix}-${tab.id}` : undefined}
          >
            <span className={styles.label}>{tab.label}</span>
            {tab.description ? <span className={styles.description}>{tab.description}</span> : null}
          </button>
        );
      })}
    </div>
  );
}
