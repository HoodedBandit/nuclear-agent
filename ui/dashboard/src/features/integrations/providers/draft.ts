import { PROVIDER_PRESETS } from "../catalog";

export function slugify(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

export function parseOptionalJson(value: string) {
  return value ? JSON.parse(value) : null;
}

export function readProviderDraft(form: HTMLFormElement, providerPreset: string) {
  const preset = PROVIDER_PRESETS.find((entry) => entry.id === providerPreset)!;
  const data = new FormData(form);
  const displayName = String(data.get("display_name") || preset.displayName).trim();
  const providerId = String(data.get("id") || "").trim() || slugify(displayName);
  const defaultModel = String(data.get("default_model") || preset.defaultModel).trim();

  return {
    preset,
    data,
    displayName,
    providerId,
    defaultModel
  };
}
