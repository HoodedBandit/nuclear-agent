export interface InstalledPluginConfig {
  id: string;
  enabled: boolean;
  trusted: boolean;
  pinned: boolean;
  source_kind?: string | null;
  source_reference?: string | null;
  source_path: string;
  install_dir: string;
  integrity_sha256?: string | null;
  reviewed_integrity_sha256?: string | null;
  reviewed_at?: string | null;
  granted_permissions?: Record<string, boolean>;
  manifest: {
    name: string;
    version: string;
    description: string;
    tools: unknown[];
    connectors: unknown[];
    provider_adapters: unknown[];
  };
}

export type PluginSourceKind = "local_path" | "git_repo" | "marketplace";

export interface PluginPermissions {
  shell: boolean;
  network: boolean;
  full_disk: boolean;
}

export interface PluginDoctorReport {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  trusted: boolean;
  runtime_ready: boolean;
  ok: boolean;
  detail: string;
  tools: number;
  connectors: number;
  provider_adapters: number;
  integrity_sha256: string;
  source_kind: PluginSourceKind;
  declared_permissions: PluginPermissions;
  granted_permissions: PluginPermissions;
  reviewed_at?: string | null;
}
