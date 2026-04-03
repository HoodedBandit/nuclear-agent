import styles from "./StatCard.module.css";

interface StatCardProps {
  label: string;
  value: string;
  detail?: string;
}

export function StatCard({ label, value, detail }: StatCardProps) {
  return (
    <article className={styles.card}>
      <div className={styles.label}>{label}</div>
      <div className={styles.value}>{value}</div>
      {detail ? <div className={styles.detail}>{detail}</div> : null}
    </article>
  );
}
