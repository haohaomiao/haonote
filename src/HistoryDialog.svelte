<script lang="ts">
  import { onMount } from 'svelte';
  import { call, listNotes } from './api';
  import type { Note, Revision } from './types';
  let {
    noteId,
    onclose,
    onrestored,
  }: { noteId: string; onclose: () => void; onrestored: () => Promise<void> } = $props();
  let dialog: HTMLDialogElement;
  let versions = $state<Revision[]>([]);
  let current = $state<Note>();
  let selected = $state<Revision>();
  let busy = $state(true);
  let more = $state(false);
  let error = $state('');
  let confirm = $state(false);
  async function load() {
    busy = true;
    error = '';
    try {
      if (!current) current = (await listNotes()).find((n) => n.id === noteId);
      const page = await call<Revision[]>('note_history', { noteId, offset: versions.length });
      versions = [...versions, ...page];
      selected ??= page.find((r) => r.id === current?.headId) ?? page[0];
      more = page.length === 50;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function restore() {
    if (!selected || !current || busy) return;
    busy = true;
    error = '';
    try {
      await call('restore_revision', { noteId, revisionId: selected.id, expected: current.headId });
      await onrestored();
      onclose();
    } catch (e) {
      error = String(e);
      confirm = false;
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    dialog.showModal();
    void load();
  });
</script>

<dialog
  bind:this={dialog!}
  class="conflict-dialog"
  oncancel={(e) => {
    e.preventDefault();
    if (!busy) onclose();
  }}
>
  <header class="dialog-header">
    <h2>历史版本</h2>
    <button class="text-button" onclick={onclose} disabled={busy}>关闭历史</button>
  </header>
  <p class="help-text">
    保留已封存的历史和设备分支，不逐字记录输入。按本机入库顺序显示，每次加载 50
    条；恢复会创建新版本，不删除其他历史。
  </p>
  {#if current?.conflicts.length}<p role="status">
      此便签存在冲突，请先处理冲突后再恢复历史。
    </p>{/if}
  <div class="history-layout">
    <div class="history-list" aria-label="版本列表">
      {#each versions as version}
        <button
          class="secondary"
          aria-pressed={selected?.id === version.id}
          disabled={busy}
          onclick={() => {
            selected = version;
            confirm = false;
          }}
        >
          {new Date(version.updatedAt).toLocaleString()} · {version.id.slice(0, 8)}
          {version.id === current?.headId ? '（当前）' : ''}{version.parents.length > 1
            ? '（合并）'
            : ''}
        </button>
      {/each}
      {#if more}<button class="text-button" onclick={load} disabled={busy}>加载更早版本</button
        >{/if}
    </div>
    {#if selected}<article class="history-preview">
        <h3>{selected.content.title || '未命名便签'}</h3>
        <p>
          颜色：{selected.content.color} · {selected.content.deleted
            ? '已删除'
            : selected.content.archived
              ? '已归档'
              : '普通便签'}
        </p>
        <pre>{selected.content.text || '（空白）'}</pre>
      </article>{/if}
  </div>
  {#if error}<p class="form-message error" role="alert">{error}</p>{/if}
  {#if busy}<p role="status">正在处理…</p>{/if}
  {#if confirm}<p>将恢复此版本的正文、标题、颜色及归档／删除状态。现有版本仍保留。</p>{/if}
  <div class="form-actions">
    <button
      class="primary"
      disabled={busy ||
        !selected ||
        !current ||
        !!current.conflicts.length ||
        selected.id === current.headId}
      onclick={() => {
        if (confirm) void restore();
        else confirm = true;
      }}>{confirm ? '确认恢复' : '恢复此版本'}</button
    >
  </div>
</dialog>

<style>
  .history-layout {
    display: grid;
    gap: 12px;
    grid-template-columns: minmax(150px, 1fr) minmax(0, 2fr);
  }
  .history-list,
  .history-preview {
    max-height: 45vh;
    overflow: auto;
  }
  .history-list button {
    display: block;
    margin-bottom: 6px;
    width: 100%;
    text-align: left;
  }
  button[aria-pressed='true'] {
    outline: 2px solid currentColor;
    outline-offset: -2px;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font: inherit;
  }
  @media (max-width: 480px) {
    .history-layout {
      grid-template-columns: 1fr;
    }
    .history-list {
      max-height: 18vh;
    }
  }
</style>
