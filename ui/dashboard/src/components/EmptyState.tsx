export function EmptyState(props: {
  title: string;
  copy: string;
}) {
  return (
    <div className="empty-state">
      <h3>{props.title}</h3>
      <p>{props.copy}</p>
    </div>
  );
}
