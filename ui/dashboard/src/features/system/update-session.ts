const UPDATE_PENDING_KEY = "nuclear-dashboard-update-pending";

export function markPendingUpdate() {
  sessionStorage.setItem(UPDATE_PENDING_KEY, "1");
}

export function clearPendingUpdate() {
  sessionStorage.removeItem(UPDATE_PENDING_KEY);
}

export function hasPendingUpdate() {
  return sessionStorage.getItem(UPDATE_PENDING_KEY) === "1";
}
