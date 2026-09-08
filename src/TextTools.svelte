<script lang="ts">
  import { Bold, Italic, Underline } from '@lucide/svelte';
  import { fonts, type NoteView, type ViewPatch } from './types';
  let {
    view,
    busy = false,
    disabled = false,
    onformat,
    onchange,
  }: {
    view: NoteView;
    busy?: boolean;
    disabled?: boolean;
    onformat: (kind: 'bold' | 'italic' | 'underline') => void;
    onchange: (patch: ViewPatch) => void;
  } = $props();
</script>

<div class="text-toolbar" role="toolbar" aria-label="正文格式工具">
  <button
    aria-label="加粗"
    title="加粗（Ctrl+B）"
    {disabled}
    onpointerdown={(e) => e.preventDefault()}
    onclick={() => onformat('bold')}><Bold size={15} /></button
  >
  <button
    aria-label="斜体"
    title="斜体（Ctrl+I）"
    {disabled}
    onpointerdown={(e) => e.preventDefault()}
    onclick={() => onformat('italic')}><Italic size={15} /></button
  >
  <button
    aria-label="下划线"
    title="下划线（Ctrl+U）"
    {disabled}
    onpointerdown={(e) => e.preventDefault()}
    onclick={() => onformat('underline')}><Underline size={15} /></button
  >
  <select
    aria-label="正文字体"
    title="整张便签的字体"
    value={view.fontFamily}
    disabled={busy}
    onchange={(e) => onchange({ fontFamily: e.currentTarget.value })}
  >
    {#each fonts as f}<option value={f.value}>{f.name}</option>{/each}
  </select>
  <select
    aria-label="正文字号"
    title="整张便签的字号"
    value={view.fontSize}
    disabled={busy}
    onchange={(e) => onchange({ fontSize: Number(e.currentTarget.value) })}
  >
    {#each [12, 14, 16, 17, 18, 20, 24, 28, 32] as size}<option value={size}>{size}</option>{/each}
  </select>
</div>
