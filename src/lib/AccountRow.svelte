<script lang="ts">
  import type { AccountView } from "./api";
  import { countdown, statusLabel, statusColor, fetchNote } from "./format";

  export let account: AccountView;

  $: note = fetchNote(account.fetch_status);
  $: nextWindow = [...account.windows].sort(
    (a, b) => a.seconds_until_reset - b.seconds_until_reset
  )[0];
</script>

<div class="row">
  <div class="head">
    <span class="dot" style="background:{statusColor(account.status)}"></span>
    <span class="label">{account.label}</span>
    <span class="plan">{account.plan.toUpperCase()}</span>
    <span class="status">{statusLabel(account.status)}</span>
    {#if note}<span class="note">{note}</span>{/if}
  </div>

  <div class="windows">
    {#each account.windows as w (w.kind)}
      <div class="win">
        <span class="wlabel">{w.label}</span>
        <div class="bar"><div class="fill" style="width:{w.usage_percent}%"></div></div>
        <span class="reset">
          {countdown(w.seconds_until_reset)}{#if w.computed}<em title="計算値">*</em>{/if}
        </span>
      </div>
    {/each}
    {#if account.windows.length === 0}
      <div class="empty">データ取得待ち…</div>
    {/if}
  </div>
</div>

<style>
  .row {
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    flex: none;
  }
  .label {
    font-weight: 600;
  }
  .plan {
    font-size: 10px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
  }
  .status {
    color: var(--muted);
  }
  .note {
    margin-left: auto;
    color: var(--bad);
    font-size: 11px;
  }
  .windows {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .win {
    display: grid;
    grid-template-columns: 92px 1fr 48px;
    align-items: center;
    gap: 8px;
    font-size: 11px;
  }
  .wlabel {
    color: var(--muted);
  }
  .bar {
    height: 6px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
  }
  .reset {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .reset em {
    color: var(--muted);
    font-style: normal;
  }
  .empty {
    font-size: 11px;
    color: var(--muted);
  }
</style>
