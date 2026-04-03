import { create } from "zustand";

export type DashboardSection =
  | "overview"
  | "chat"
  | "integrations"
  | "operations"
  | "system";

interface DashboardStoreState {
  activeSection: DashboardSection;
  selectedSessionId: string | null;
  activeRunSessionId: string | null;
  setActiveSection: (section: DashboardSection) => void;
  setSelectedSessionId: (sessionId: string | null) => void;
  setActiveRunSessionId: (sessionId: string | null) => void;
}

export const useDashboardStore = create<DashboardStoreState>((set) => ({
  activeSection: "overview",
  selectedSessionId: null,
  activeRunSessionId: null,
  setActiveSection: (activeSection) => set({ activeSection }),
  setSelectedSessionId: (selectedSessionId) => set({ selectedSessionId }),
  setActiveRunSessionId: (activeRunSessionId) => set({ activeRunSessionId })
}));
