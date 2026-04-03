export function fmtDate(value?: string | null) {
  if (!value) {
    return "Unavailable";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(date);
}

export function fmtCount(value: number) {
  return new Intl.NumberFormat().format(value);
}

export function fmtDurationFrom(value?: string | null) {
  if (!value) {
    return "Unknown";
  }

  const then = new Date(value).getTime();
  if (Number.isNaN(then)) {
    return value;
  }
  const diffMinutes = Math.max(0, Math.round((Date.now() - then) / 60_000));
  if (diffMinutes < 1) {
    return "just now";
  }
  if (diffMinutes < 60) {
    return `${diffMinutes}m ago`;
  }
  const diffHours = Math.round(diffMinutes / 60);
  if (diffHours < 24) {
    return `${diffHours}h ago`;
  }
  return `${Math.round(diffHours / 24)}d ago`;
}

export function startCase(value: string) {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (match) => match.toUpperCase());
}
