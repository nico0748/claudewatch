// Thin wrappers over the Tauri IPC surface.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Plan = "pro" | "max";
export type Browser = "chrome" | "brave" | "firefox" | "safari";
export type AccountStatus = "available" | "in_window" | "limited";
export type FetchStatus = "ok" | "stale" | "auth_required" | "error";
export type WindowKind = "five_hour" | "weekly" | "weekly_sonnet";

export interface WindowView {
  kind: WindowKind;
  label: string;
  usage_percent: number;
  resets_at: string;
  seconds_until_reset: number;
  computed: boolean;
}

export interface AccountView {
  id: string;
  label: string;
  plan: Plan;
  browser: Browser;
  status: AccountStatus;
  fetch_status: FetchStatus;
  windows: WindowView[];
  last_fetched_at: string | null;
}

export interface ProfileInfo {
  id: string;
  display_name: string;
}

export interface NewAccount {
  label: string;
  plan: Plan;
  browser: Browser;
  browser_profile: string;
  timezone: string;
  weekly_reset_weekday: number;
  weekly_reset_time: string; // "HH:MM"
}

export const api = {
  getAccounts: () => invoke<AccountView[]>("get_accounts"),
  addAccount: (account: NewAccount) => invoke<AccountView>("add_account", { account }),
  removeAccount: (id: string) => invoke<void>("remove_account", { id }),
  detectProfiles: (browser: Browser) =>
    invoke<ProfileInfo[]>("detect_browser_profiles", { browser }),
  refreshNow: () => invoke<void>("refresh_now"),
  getSettings: () => invoke<Record<string, unknown>>("get_settings"),
  updateSettings: (settings: Record<string, unknown>) =>
    invoke<void>("update_settings", { settings }),
};

export function onAccountUpdated(cb: (a: AccountView) => void): Promise<UnlistenFn> {
  return listen<AccountView>("account_updated", (e) => cb(e.payload));
}
