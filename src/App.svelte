<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { api, onAccountUpdated, type AccountView } from "./lib/api";
  import AccountRow from "./lib/AccountRow.svelte";

  let accounts: AccountView[] = [];
  let loading = true;
  let unlisten: UnlistenFn | null = null;

  $: availableCount = accounts.filter((a) => a.status === "available").length;

  function upsert(a: AccountView) {
    const i = accounts.findIndex((x) => x.id === a.id);
    if (i >= 0) accounts[i] = a;
    else accounts = [...accounts, a];
    accounts = accounts;
  }

  onMount(async () => {
    try {
      accounts = await api.getAccounts();
    } finally {
      loading = false;
    }
    unlisten = await onAccountUpdated(upsert);
  });

  onDestroy(() => unlisten?.());

  async function refresh() {
    await api.refreshNow();
  }
</script>

<main>
  <header>
    <div class="brand">claudewatch</div>
    <div class="summary">利用可 <strong>{availableCount}</strong>/{accounts.length || 0}</div>
    <button class="refresh" on:click={refresh} title="今すぐ更新">⟳</button>
  </header>

  <section class="list">
    {#if loading}
      <div class="placeholder">読み込み中…</div>
    {:else if accounts.length === 0}
      <div class="placeholder">
        アカウントが未登録です。<br />設定からブラウザとアカウントを追加してください。
      </div>
    {:else}
      {#each accounts as a (a.id)}
        <AccountRow account={a} />
      {/each}
    {/if}
  </section>
</main>

<style>
  :global(:root) {
    --bg: #1c1c1e;
    --fg: #f2f2f4;
    --muted: #9a9aa2;
    --border: #333338;
    --accent: #c96442;
    --ok: #3fb950;
    --warn: #d29922;
    --bad: #f85149;
  }
  :global(body) {
    margin: 0;
    font-family: -apple-system, system-ui, sans-serif;
    background: var(--bg);
    color: var(--fg);
  }
  main {
    width: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .brand {
    font-weight: 700;
  }
  .summary {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .summary strong {
    color: var(--ok);
  }
  .refresh {
    background: none;
    border: 1px solid var(--border);
    color: var(--fg);
    border-radius: 6px;
    cursor: pointer;
    width: 26px;
    height: 26px;
  }
  .refresh:hover {
    border-color: var(--accent);
  }
  .placeholder {
    padding: 24px 12px;
    text-align: center;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.6;
  }
</style>
