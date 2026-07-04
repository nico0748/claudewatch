// Small display helpers.
import type { AccountStatus, FetchStatus } from "./api";

/** Seconds -> "H:MM" (or "0:MM" under an hour). */
export function countdown(seconds: number): string {
  if (seconds <= 0) return "0:00";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}:${m.toString().padStart(2, "0")}`;
}

export function statusLabel(s: AccountStatus): string {
  switch (s) {
    case "available":
      return "利用可";
    case "in_window":
      return "枠消化中";
    case "limited":
      return "上限到達";
  }
}

export function statusColor(s: AccountStatus): string {
  switch (s) {
    case "available":
      return "var(--ok)";
    case "in_window":
      return "var(--warn)";
    case "limited":
      return "var(--bad)";
  }
}

export function fetchNote(f: FetchStatus): string | null {
  switch (f) {
    case "ok":
      return null;
    case "stale":
      return "更新待ち";
    case "auth_required":
      return "要再ログイン";
    case "error":
      return "取得エラー";
  }
}
