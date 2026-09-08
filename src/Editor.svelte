<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    Pin,
    X,
    Ellipsis,
    Archive,
    Trash2,
    Check,
    AlertTriangle,
    PanelLeft,
    Plus,
    Eye,
    Pencil,
    Grip,
  } from '@lucide/svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Menu } from '@tauri-apps/api/menu';
  import { LogicalPosition } from '@tauri-apps/api/dpi';
  import { broadcast, call, listNotes, native, on, saveNote } from './api';
  import {
    colors,
    emptyContent,
    fonts,
    type Content,
    type Note,
    type NoteView,
    type ViewPatch,
  } from './types';
  import RichEditor from './RichEditor.svelte';
  import TextTools from './TextTools.svelte';

  let {
    noteId,
    standalone = false,
    onclose = () => {},
  }: { noteId: string; standalone?: boolean; onclose?: () => void } = $props();
  let note = $state<Note | null>(null);
  let text = $state('');
  let noteTitle = $state('');
  let titleInput = $state<HTMLInputElement>();
  let color = $state<Content['color']>('butter');
  let menu = $state(false);
  let pinned = $state(false);
  let message = $state('正在打开…');
  let failure = $state('');
  let dirty = false;
  let composing = false;
  let timer: ReturnType<typeof setTimeout>;
  let saving: Promise<void> | null = null;
  let editor = $state<HTMLTextAreaElement>();
  let richEditor = $state<RichEditor>();
  let textMenu = $state<{ x: number; y: number } | null>(null);
  let sourceSelection: { start: number; end: number } | undefined;
  let closing = false;
  let view = $state<NoteView>({
    collapsed: false,
    opacity: 100,
    markdown: true,
    fontFamily: 'sans',
    fontSize: 17,
  });
  let viewBusy = $state(false);
  let width = $state(310);
  let height = $state(330);
  let opacity = $state(100);
  let dragStart: { x: number; y: number } | null = null;
  let contextMenu: Menu | undefined;
  const heading = $derived(
    noteTitle.trim() ||
      text
        .trim()
        .split('\n')[0]
        .replace(/^#{1,6}\s+/, '') ||
      'haonote',
  );
  const font = $derived(fonts.find((f) => f.value === view.fontFamily)?.css || fonts[0].css);

  async function editTitle() {
    await toggleMenu(true);
    await tick();
    titleInput?.focus();
    titleInput?.select();
  }

  function formatText(kind: 'bold' | 'italic' | 'underline') {
    if (view.markdown) {
      richEditor?.format(kind);
      textMenu = null;
      return;
    }
    if (
      !editor ||
      !note ||
      composing ||
      view.markdown ||
      note.content.deleted ||
      note.content.archived
    )
      return;
    if (sourceSelection) editor.setSelectionRange(sourceSelection.start, sourceSelection.end);
    const [left, right] =
      kind === 'bold' ? ['**', '**'] : kind === 'italic' ? ['_', '_'] : ['<u>', '</u>'];
    let start = editor.selectionStart;
    let end = editor.selectionEnd;
    const selected = text.slice(start, end) || '文字';
    const wrapped =
      text.slice(Math.max(0, start - left.length), start) === left &&
      text.slice(end, end + right.length) === right;
    const replacement = wrapped ? selected : left + selected + right;
    if (wrapped) {
      start -= left.length;
      end += right.length;
    }
    editor.focus();
    editor.setSelectionRange(start, end);
    // insertText preserves the WebView's native undo history; setRangeText is the fallback.
    if (!document.execCommand('insertText', false, replacement))
      editor.setRangeText(replacement, start, end, 'end');
    text = editor.value;
    const selectionStart = start + (wrapped ? 0 : left.length);
    editor.setSelectionRange(selectionStart, selectionStart + selected.length);
    sourceSelection = { start: selectionStart, end: selectionStart + selected.length };
    textMenu = null;
    changed();
  }

  function captureTextSelection() {
    if (view.markdown) richEditor?.captureSelection();
    else if (editor) sourceSelection = { start: editor.selectionStart, end: editor.selectionEnd };
  }
  async function showTextMenu(event: MouseEvent) {
    event.preventDefault();
    captureTextSelection();
    menu = false;
    if (!native || !standalone) {
      textMenu = {
        x: Math.max(0, Math.min(event.clientX, innerWidth - 260)),
        y: Math.max(0, Math.min(event.clientY, innerHeight - 65)),
      };
      return;
    }
    try {
      await contextMenu?.close();
      const editable =
        !!note &&
        !note.content.deleted &&
        !note.content.archived &&
        (!view.markdown || richEditor?.canEdit());
      contextMenu = await Menu.new({
        items: [
          ...(['bold', 'italic', 'underline'] as const).map((kind, i) => ({
            id: `${noteId}:format-${kind}`,
            text: ['加粗', '斜体', '下划线'][i],
            checked: view.markdown ? richEditor?.active(kind) : false,
            enabled: !!editable,
            action: () => formatText(kind),
          })),
          { item: 'Separator' },
          {
            text: '字体（整张便签）',
            items: fonts.map((f) => ({
              text: f.name,
              checked: view.fontFamily === f.value,
              action: () => {
                void changeView({ fontFamily: f.value });
              },
            })),
          },
          {
            text: '字号（整张便签）',
            items: [12, 14, 16, 17, 18, 20, 24, 28, 32].map((size) => ({
              text: String(size),
              checked: view.fontSize === size,
              action: () => {
                void changeView({ fontSize: size });
              },
            })),
          },
        ],
      });
      await contextMenu.popup(new LogicalPosition(event.clientX, event.clientY));
    } catch (e) {
      failure = String(e);
    }
  }

  async function changeView(patch: ViewPatch) {
    if (viewBusy) return;
    viewBusy = true;
    try {
      // Switching modes and collapsing must not discard an outstanding edit.
      await flush();
      if (native && standalone) view = await call<NoteView>('update_note_view', { patch });
      else {
        view = { ...view, ...patch };
        localStorage.setItem(`qingnote-view-${noteId}`, JSON.stringify(view));
      }
      opacity = view.opacity;
      if (view.collapsed) menu = false;
    } catch (error) {
      opacity = view.opacity;
      failure = String(error);
    } finally {
      viewBusy = false;
    }
  }
  async function toggleCollapsed() {
    if (composing) return;
    await changeView({ collapsed: !view.collapsed });
  }
  async function toggleMenu(forceOpen = false) {
    captureTextSelection();
    if (view.collapsed) await changeView({ collapsed: false });
    if (native && standalone) {
      const window = getCurrentWindow();
      const size = (await window.innerSize()).toLogical(await window.scaleFactor());
      width = Math.round(size.width);
      height = Math.round(size.height);
    }
    menu = forceOpen || !menu;
  }
  async function showContextMenu(event: MouseEvent) {
    event.preventDefault();
    if (!note || viewBusy) return;
    try {
      if (!native || !standalone) {
        await toggleMenu(true);
        return;
      }
      await contextMenu?.close();
      const item = (id: string, text: string, action: () => void) => ({
        id: `${noteId}:${id}`,
        text,
        action,
      });
      contextMenu = await Menu.new({
        items: [
          item('collapse', view.collapsed ? '展开便签主体' : '折叠便签主体', () => {
            void toggleCollapsed();
          }),
          item('title', '编辑标题…（F2）', () => {
            void editTitle().catch((e) => (failure = String(e)));
          }),
          {
            ...item('pin', '窗口置顶', () => {
              void pin();
            }),
            checked: pinned,
          },
          {
            ...item('markdown', '排版编辑', () => {
              void changeView({ markdown: !view.markdown });
            }),
            checked: view.markdown,
          },
          { item: 'Separator' },
          {
            text: '便签颜色',
            items: colors.map((c) => ({
              ...item(`color-${c.value}`, c.name, () => {
                color = c.value;
                changed();
              }),
              checked: color === c.value,
            })),
          },
          {
            text: '不透明度',
            items: [100, 85, 70, 50, 30].map((value) => ({
              ...item(`opacity-${value}`, `${value}%`, () => {
                void changeView({ opacity: value });
              }),
              checked: view.opacity === value,
            })),
          },
          {
            text: '便签大小',
            items: [
              item('small', '小 · 280 × 280', () => {
                void changeView({ width: 280, height: 280 });
              }),
              item('medium', '标准 · 310 × 330', () => {
                void changeView({ width: 310, height: 330 });
              }),
              item('large', '大 · 420 × 460', () => {
                void changeView({ width: 420, height: 460 });
              }),
              item('custom', '自定义大小…', () => {
                void toggleMenu(true).catch((e) => (failure = String(e)));
              }),
            ],
          },
          item('options', '更多设置…', () => {
            void toggleMenu(true).catch((e) => (failure = String(e)));
          }),
          { item: 'Separator' },
          item('new', '新建便签', () => {
            void add();
          }),
          item('library', '打开便签列表', () => {
            void openLibrary();
          }),
          item('archive', '归档便签', () => {
            void action('archive');
          }),
          item('delete', '移到回收站', () => {
            void action('delete');
          }),
          { item: 'Separator' },
          item('close', '收起窗口（不删除）', () => {
            void close();
          }),
        ],
      });
      await contextMenu.popup(new LogicalPosition(event.clientX, event.clientY));
    } catch (error) {
      failure = String(error);
    }
  }
  function drag(event: PointerEvent) {
    if (!dragStart || !event.buttons) return;
    if (Math.hypot(event.clientX - dragStart.x, event.clientY - dragStart.y) < 4) return;
    dragStart = null;
    if (native && standalone)
      void getCurrentWindow()
        .startDragging()
        .catch((e) => (failure = String(e)));
  }

  async function refresh() {
    if (dirty || saving) return;
    const current = (await listNotes()).find((n) => n.id === noteId);
    // Input may have started while the IPC read was in flight.
    if (dirty || saving || composing) return;
    if (current) {
      note = current;
      text = current.content.text;
      noteTitle = current.content.title || '';
      color = current.content.color;
      message = '已保存在本机';
    }
  }
  function changed() {
    dirty = true;
    message = '正在保存…';
    failure = '';
    clearTimeout(timer);
    if (!composing)
      timer = setTimeout(() => {
        void flush().catch(() => {});
      }, 250);
  }
  async function flush(): Promise<void> {
    // Let the rich editor finish pending composition/DOM updates before switching or closing.
    if (view.markdown) await new Promise<void>((resolve) => setTimeout(resolve, 0));
    clearTimeout(timer);
    if (saving) {
      await saving;
      if (dirty) return flush();
      return;
    }
    if (!note || !dirty) return;
    saving = (async () => {
      while (dirty && note) {
        const content = { ...note.content, text, color, title: noteTitle };
        dirty = false;
        try {
          note = await saveNote(note, content);
          message = '已保存在本机';
        } catch (error) {
          dirty = true;
          failure = String(error);
          message = '尚未保存，请勿关闭';
          throw error;
        }
      }
    })();
    try {
      await saving;
    } finally {
      saving = null;
    }
  }
  async function close() {
    if (closing) return;
    closing = true;
    try {
      await flush();
      if (standalone && native) await call('close_note');
      else onclose();
    } catch (error) {
      failure = String(error);
    } finally {
      closing = false;
    }
  }
  async function action(kind: 'archive' | 'delete') {
    try {
      await flush();
      if (!note) return;
      note = await saveNote(note, {
        ...note.content,
        archived: kind === 'archive' || note.content.archived,
        deleted: kind === 'delete',
      });
      await close();
    } catch (error) {
      failure = String(error);
    }
  }
  async function pin() {
    try {
      const desired = !pinned;
      await call('pin_note', { pinned: desired });
      pinned = desired;
    } catch (error) {
      failure = String(error);
    }
  }
  async function add() {
    try {
      await flush();
      const n = await saveNote(null, emptyContent());
      await call('open_note', { noteId: n.id });
    } catch (error) {
      failure = String(error);
    }
  }
  async function openLibrary() {
    try {
      await flush();
      await call('open_library');
    } catch (e) {
      failure = String(e);
    }
  }
  onMount(() => {
    let disposed = false;
    const off: (() => void)[] = [];
    async function initialize() {
      try {
        await refresh();
        if (native && standalone) {
          pinned = await call<boolean>('get_pinned');
          view = await call<NoteView>('get_note_view');
        } else {
          const saved = localStorage.getItem(`qingnote-view-${noteId}`);
          if (saved) view = { ...view, ...JSON.parse(saved) };
        }
        opacity = view.opacity;
        const listeners: [string, (payload: unknown) => void][] = [
          [
            'notes-changed',
            () => {
              void refresh().catch((e) => (failure = String(e)));
            },
          ],
          [
            'request-close',
            () => {
              void close();
            },
          ],
          [
            'flush-editors',
            (token) => {
              void flush()
                .then(() => broadcast('editor-flushed', { token, noteId, ok: true }))
                .catch(() => broadcast('editor-flushed', { token, noteId, ok: false }));
            },
          ],
        ];
        for (const [event, handler] of listeners) {
          const unlisten = await on(event, handler);
          if (disposed) unlisten();
          else off.push(unlisten);
        }
        editor?.focus();
      } catch (e) {
        failure = String(e);
      }
    }
    void initialize();
    return () => {
      disposed = true;
      clearTimeout(timer);
      off.forEach((f) => f());
      void contextMenu?.close().catch(() => {});
    };
  });
</script>

<svelte:window
  onpointerdown={(event) => {
    if (textMenu && event.target instanceof Element && !event.target.closest('.text-context-menu'))
      textMenu = null;
    if (
      menu &&
      event.target instanceof Element &&
      !event.target.closest('.editor-options, [data-options-trigger]')
    )
      menu = false;
  }}
  onkeydown={(event) => {
    if (event.key === 'F2') {
      event.preventDefault();
      void editTitle().catch((e) => (failure = String(e)));
    }
    if ((event.ctrlKey || event.metaKey) && event.target === editor) {
      const kind = ({ b: 'bold', i: 'italic', u: 'underline' } as const)[
        event.key.toLowerCase() as 'b' | 'i' | 'u'
      ];
      if (kind) {
        event.preventDefault();
        formatText(kind);
      }
    }
    if ((event.ctrlKey || event.metaKey) && event.key === 's') {
      event.preventDefault();
      void flush().catch(() => {});
    }
    if (event.key === 'Escape') {
      if (textMenu) textMenu = null;
      else if (menu) menu = false;
      else void close();
    }
  }}
  onblur={() => {
    menu = false;
    textMenu = null;
    if (!composing) void flush().catch(() => {});
  }}
/>

<section
  class:standalone
  class:collapsed={view.collapsed}
  class="editor note-{color}"
  aria-label="便签编辑器"
  style:opacity={opacity / 100}
>
  <header
    class="editor-bar"
    role="toolbar"
    tabindex="-1"
    aria-label="便签标题工具栏"
    oncontextmenu={showContextMenu}
  >
    <button
      class="icon-button"
      title="新建便签"
      aria-label="新建便签"
      onclick={add}
      disabled={!native}><Plus size={18} /></button
    >
    <button
      class="drag-handle"
      title="拖动移动 · 双击折叠 / 展开 · 右键常用操作"
      aria-label="便签标题栏"
      aria-expanded={!view.collapsed}
      onpointerdown={(event) => {
        if (event.button === 0) dragStart = { x: event.clientX, y: event.clientY };
      }}
      onpointermove={drag}
      onpointerup={() => (dragStart = null)}
      onpointercancel={() => (dragStart = null)}
      ondblclick={toggleCollapsed}
      onkeydown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          void toggleCollapsed();
        }
      }}
    >
      <span>{heading}</span>
    </button>
    <button
      class:active={pinned}
      class="icon-button"
      title="窗口置顶"
      aria-label="窗口置顶"
      onclick={pin}
      disabled={!native}><Pin size={16} /></button
    >
    <button
      class="icon-button"
      title="便签选项"
      aria-label="便签选项"
      data-options-trigger
      aria-expanded={menu}
      onclick={() => toggleMenu().catch((e) => (failure = String(e)))}
      ><Ellipsis size={19} /></button
    >
    <button class="icon-button" title="收起便签（不删除）" aria-label="收起便签" onclick={close}
      ><X size={17} /></button
    >
  </header>
  {#if !view.collapsed}
    {#if menu}
      <div class="editor-options">
        <TextTools
          {view}
          busy={viewBusy}
          disabled={!note ||
            note.content.deleted ||
            note.content.archived ||
            (view.markdown && !richEditor?.canEdit())}
          onformat={formatText}
          onchange={changeView}
        />
        <label class="title-control"
          >标题<input
            bind:this={titleInput}
            bind:value={noteTitle}
            oninput={changed}
            maxlength="160"
            aria-label="便签标题"
            placeholder="留空时使用正文第一行"
            disabled={!note || note.content.deleted || note.content.archived}
            oncompositionstart={() => {
              composing = true;
              clearTimeout(timer);
            }}
            oncompositionend={() => {
              composing = false;
              changed();
            }}
            onkeydown={(event) => {
              if (event.key === 'Enter' && !event.isComposing) {
                menu = false;
                void flush().catch(() => {});
              }
            }}
          /></label
        >
        <div class="color-row" aria-label="便签颜色">
          {#each colors as c}<button
              class="swatch note-{c.value}"
              class:selected={color === c.value}
              title={c.name}
              aria-label={c.name}
              onclick={() => {
                color = c.value;
                changed();
              }}
              >{#if color === c.value}<Check size={14} />{/if}</button
            >{/each}
        </div>
        <div class="option-row">
          <button
            title={view.markdown ? '编辑 Markdown 源码' : '排版编辑'}
            aria-label={view.markdown ? '编辑 Markdown 源码' : '排版编辑'}
            disabled={viewBusy}
            onclick={() => {
              void changeView({ markdown: !view.markdown });
            }}
          >
            {#if view.markdown}<Pencil size={16} />{:else}<Eye size={16} />{/if}
            <span>Markdown</span>
          </button>
          <button title="归档" aria-label="归档便签" onclick={() => action('archive')}
            ><Archive size={16} /></button
          >
          <button title="移到回收站" aria-label="删除便签" onclick={() => action('delete')}
            ><Trash2 size={16} /></button
          >
          {#if native}<button title="便签列表" aria-label="便签列表" onclick={openLibrary}
              ><PanelLeft size={16} /></button
            >{/if}
        </div>
        <label class="opacity-control"
          >不透明度 <input
            aria-label="便签不透明度"
            type="range"
            min="30"
            max="100"
            step="5"
            bind:value={opacity}
            disabled={viewBusy}
            onchange={() => changeView({ opacity })}
          /><output>{opacity}%</output></label
        >
        {#if native && standalone}
          <form
            class="size-controls"
            onsubmit={(event) => {
              event.preventDefault();
              void changeView({ width, height });
            }}
          >
            <label
              >宽 <input
                aria-label="便签宽度"
                type="number"
                min="240"
                max="1600"
                step="1"
                required
                bind:value={width}
              /></label
            >
            <label
              >高 <input
                aria-label="便签高度"
                type="number"
                min="200"
                max="1200"
                step="1"
                required
                bind:value={height}
              /></label
            >
            <button type="submit" disabled={viewBusy}>应用大小</button>
          </form>
        {/if}
      </div>
    {/if}
    {#if note?.conflicts.length}<div class="inline-warning">
        <AlertTriangle size={14} />有其他设备的修改，请到列表处理冲突。
      </div>{/if}
    {#if note?.content.deleted || note?.content.archived}<div class="inline-warning">
        这张便签已归档或移到回收站。
      </div>{/if}
    {#if view.markdown}
      <div class="rich-container" style:font-size="{view.fontSize}px" style:font-family={font}>
        <RichEditor
          bind:this={richEditor}
          source={text}
          disabled={!note || note.content.deleted || note.content.archived}
          onchange={(value) => {
            text = value;
            changed();
          }}
          oncomposition={(active) => {
            composing = active;
            if (active) clearTimeout(timer);
            else changed();
          }}
          oncontextmenu={showTextMenu}
          onsource={() => {
            void changeView({ markdown: false });
          }}
        />
      </div>
    {:else}<textarea
        bind:this={editor!}
        bind:value={text}
        oninput={changed}
        oncontextmenu={showTextMenu}
        onselect={() => {
          if (editor) sourceSelection = { start: editor.selectionStart, end: editor.selectionEnd };
        }}
        oncompositionstart={() => {
          composing = true;
          clearTimeout(timer);
        }}
        oncompositionend={() => {
          composing = false;
          changed();
        }}
        aria-label="便签内容"
        placeholder="在这里，记下一点什么…"
        spellcheck="false"
        disabled={!note || note.content.deleted || note.content.archived}
        style:font-size="{view.fontSize}px"
        style:font-family={font}></textarea>{/if}
    {#if failure}<div class="editor-error" role="alert">
        {failure}<button onclick={() => flush().catch(() => {})}>重试保存</button>
      </div>{/if}
    <footer class="editor-footer">
      <span><span class="status-dot"></span>{message}</span>
      <button
        class="mode-toggle"
        disabled={viewBusy}
        aria-label={view.markdown ? '切换到 Markdown 源码' : '切换到排版编辑'}
        onclick={() => changeView({ markdown: !view.markdown })}
        >{view.markdown ? '源码' : '排版'}</button
      >
      <span>{text.length} 字</span>
    </footer>
    {#if native && standalone}
      <button
        class="resize-handle"
        aria-label="拖动调整便签大小"
        title="拖动调整大小"
        onpointerdown={(event) => {
          if (event.button === 0)
            void getCurrentWindow()
              .startResizeDragging('SouthEast')
              .catch((e) => (failure = String(e)));
        }}><Grip size={12} /></button
      >
    {/if}
  {/if}
</section>
{#if textMenu}
  <div
    class="text-context-menu"
    style:left="{textMenu.x}px"
    style:top="{textMenu.y}px"
    role="dialog"
    aria-label="选区格式"
  >
    <TextTools
      {view}
      busy={viewBusy}
      disabled={!note ||
        note.content.deleted ||
        note.content.archived ||
        (view.markdown && !richEditor?.canEdit())}
      onformat={formatText}
      onchange={changeView}
    />
  </div>
{/if}
