<script lang="ts">
  import { onMount } from 'svelte';
  import { Editor } from '@tiptap/core';
  import DOMPurify from 'dompurify';
  import { richExtensions, sameMarkdownMeaning, unsupportedMarkdown } from './rich-text';
  import { renderMarkdown } from './markdown';

  let {
    source,
    disabled = false,
    onchange,
    oncomposition,
    oncontextmenu,
    onsource,
  }: {
    source: string;
    disabled?: boolean;
    onchange: (source: string) => void;
    oncomposition: (active: boolean) => void;
    oncontextmenu: (event: MouseEvent) => void;
    onsource: () => void;
  } = $props();
  let host: HTMLDivElement;
  let instance = $state<Editor>();
  let warning = $state('');
  let applied = '';
  let selection: { from: number; to: number } | undefined;
  let focusedOnce = false;
  let finishing: ReturnType<typeof setTimeout>;

  function publish() {
    if (!instance || warning) return;
    applied = instance.getMarkdown();
    onchange(applied);
  }
  function load(source: string) {
    if (!instance) return;
    applied = source;
    warning = '';
    try {
      if (unsupportedMarkdown(source)) throw new Error('unsupported');
      instance.commands.setContent(source, { contentType: 'markdown', emitUpdate: false });
      if (!sameMarkdownMeaning(source, instance.getMarkdown())) throw new Error('lossy');
    } catch {
      warning = '这段内容含暂不支持直接编辑的语法。源码已保留，请切换源码修改。';
    }
    instance.setEditable(!disabled && !warning, false);
  }
  export function captureSelection() {
    if (instance)
      selection = { from: instance.state.selection.from, to: instance.state.selection.to };
  }
  export function format(kind: 'bold' | 'italic' | 'underline') {
    if (!instance || disabled || warning) return;
    let command = instance.chain().focus();
    if (selection) command = command.setTextSelection(selection);
    if (kind === 'bold') command.toggleBold().run();
    else if (kind === 'italic') command.toggleItalic().run();
    else command.toggleUnderline().run();
    captureSelection();
  }
  export function canEdit() {
    return !!instance && !disabled && !warning;
  }
  export function active(kind: string) {
    return instance?.isActive(kind) ?? false;
  }

  onMount(() => {
    instance = new Editor({
      element: host,
      extensions: richExtensions(),
      content: '',
      injectCSS: false,
      editorProps: {
        attributes: {
          role: 'textbox',
          'aria-label': '排版正文',
          'aria-multiline': 'true',
          spellcheck: 'false',
        },
        // Pasted HTML is schema-filtered too, but remove resource-loading attributes first.
        transformPastedHTML: (html) =>
          DOMPurify.sanitize(html, {
            FORBID_TAGS: ['img', 'svg', 'math', 'iframe', 'video', 'audio', 'style'],
            FORBID_ATTR: ['src', 'srcset', 'style'],
          }),
        handleDOMEvents: {
          contextmenu: (_view, event) => {
            captureSelection();
            oncontextmenu(event);
            return true;
          },
          click: (_view, event) => {
            if ((event.target as Element).closest('a')) {
              event.preventDefault();
              return true;
            }
            return false;
          },
          auxclick: (_view, event) => {
            if ((event.target as Element).closest('a')) {
              event.preventDefault();
              return true;
            }
            return false;
          },
          compositionstart: () => {
            oncomposition(true);
            return false;
          },
          compositionend: () => {
            finishing = setTimeout(() => {
              publish();
              oncomposition(false);
            }, 0);
            return false;
          },
        },
      },
      onUpdate: publish,
    });
    load(source);
    return () => {
      clearTimeout(finishing);
      instance?.destroy();
    };
  });
  $effect(() => {
    if (instance && source !== applied) load(source);
    instance?.setEditable(!disabled && !warning, false);
    if (instance && !disabled && !warning && !focusedOnce) {
      focusedOnce = true;
      instance.commands.focus('end');
    }
  });
</script>

{#if warning}
  <div class="rich-warning" role="status">
    {warning}<button onclick={onsource}>编辑源码</button>
  </div>
  <div class="markdown-preview rich-fallback" aria-label="Markdown 预览">
    {@html renderMarkdown(source)}
  </div>
{/if}
<div class="rich-host markdown-preview" class:unavailable={!!warning} bind:this={host!}></div>
