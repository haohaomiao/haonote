<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { X, GitMerge } from '@lucide/svelte';
  import { call } from './api';
  import type { Content, Note } from './types';
  let { note, onclose }: { note: Note; onclose: () => void } = $props();
  let dialog: HTMLDialogElement;
  // Keep the opened snapshot stable; Rust rejects resolution if new heads arrive.
  const snapshot = untrack(() => note);
  const versions = [
    { id: snapshot.headId, content: snapshot.content, updatedAt: snapshot.updatedAt },
    ...snapshot.conflicts,
  ];
  let merged = $state([...new Set(versions.map((v) => v.content.text))].join('\n\n'));
  let mergedTitle = $state(snapshot.content.title || '');
  let selected = $state<Content>({ ...snapshot.content, archived: false, deleted: false });
  let error = $state('');
  let busy = $state(false);
  onMount(() => dialog.showModal());
  async function resolve() {
    busy = true;
    try {
      await call('resolve_note', {
        noteId: note.id,
        heads: versions.map((v) => v.id),
        content: { ...selected, text: merged, title: mergedTitle },
      });
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
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
    <div>
      <h2>处理便签冲突</h2>
    </div>
    <button class="icon-button" aria-label="关闭冲突处理" onclick={onclose} disabled={busy}
      ><X size={20} /></button
    >
  </header>
  <p class="help-text">不同设备修改了同一张便签。可以采用某个版本，也可以在下方整理合并内容。</p>
  <div class="conflict-versions">
    {#each versions as version, i}<article>
        <header>
          <span>版本 {i + 1} · {new Date(version.updatedAt).toLocaleString('zh-CN')}</span><button
            class="text-button"
            onclick={() => {
              selected = version.content;
              merged = version.content.text;
              mergedTitle = version.content.title || '';
            }}>采用此版本</button
          >
        </header>
        {#if version.content.title}<h3>{version.content.title}</h3>{/if}
        <p>{version.content.text || '（空白）'}</p>
        {#if version.content.deleted}<small>此版本已删除</small
          >{:else if version.content.archived}<small>此版本已归档</small>{/if}
      </article>{/each}
  </div>
  <label class="field">最终标题<input bind:value={mergedTitle} maxlength="160" /></label>
  <label class="field">最终内容<textarea bind:value={merged} rows="6"></textarea></label>
  <label class="checkbox-row"
    ><input type="checkbox" bind:checked={selected.deleted} />保留删除状态（移到回收站）</label
  >
  {#if error}<p class="form-message error" role="alert">{error}</p>{/if}
  <div class="form-actions">
    <button class="primary" onclick={resolve} disabled={busy}
      ><GitMerge size={17} />保存合并结果</button
    ><button class="secondary" onclick={onclose} disabled={busy}>稍后处理</button>
  </div>
</dialog>
