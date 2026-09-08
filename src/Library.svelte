<script lang="ts">
  import { onMount } from 'svelte';
  import {
    StickyNote,
    Search,
    Plus,
    Archive,
    Trash2,
    Settings,
    Cloud,
    RefreshCw,
    ArrowUpRight,
    RotateCcw,
    AlertTriangle,
    LogOut,
    Download,
    Upload,
    X,
  } from '@lucide/svelte';
  import { broadcast, call, listNotes, native, on, saveNote, syncStatus } from './api';
  import { emptyContent, shortDate, title, type Note, type SyncStatus } from './types';
  import Editor from './Editor.svelte';
  import SettingsDialog from './SettingsDialog.svelte';
  import ConflictDialog from './ConflictDialog.svelte';

  type Filter = 'active' | 'archived' | 'deleted' | 'conflicts';
  let notes = $state<Note[]>([]);
  let filter = $state<Filter>('active');
  let query = $state('');
  let settingsOpen = $state(false);
  let previewNote = $state<string | null>(null);
  let conflict = $state<Note | null>(null);
  let status = $state<SyncStatus>({
    running: false,
    lastSuccess: null,
    message: '内容保存在本机',
    pending: 0,
  });
  let notice = $state('');
  let busy = $state(false);
  let quitting = false;
  let searchInput: HTMLInputElement;
  const labels: Record<Filter, string> = {
    active: '我的便签',
    archived: '归档',
    deleted: '回收站',
    conflicts: '待处理冲突',
  };
  const descriptions: Record<Filter, string> = {
    active: '把想法放在眼前，把琐事留在这里。',
    archived: '暂时收好的事情，随时可以找回来。',
    deleted: '删除的便签留在这里，可以随时恢复。',
    conflicts: '不同设备的修改都在，选一个版本或把它们合并。',
  };
  const belongs = (n: Note, f: Filter) =>
    f === 'conflicts'
      ? n.conflicts.length > 0
      : f === 'deleted'
        ? n.content.deleted
        : f === 'archived'
          ? n.content.archived && !n.content.deleted
          : !n.content.archived && !n.content.deleted;
  let visible = $derived(
    notes.filter(
      (n) =>
        belongs(n, filter) &&
        `${title(n)}\n${n.content.text}`.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
    ),
  );
  let count = $derived(notes.filter((n) => belongs(n, 'active')).length);
  let conflictCount = $derived(notes.filter((n) => n.conflicts.length).length);

  async function refresh() {
    notes = await listNotes();
    status = await syncStatus();
  }
  async function task(fn: () => Promise<void>) {
    if (busy) return;
    busy = true;
    notice = '';
    try {
      await fn();
    } catch (e) {
      notice = String(e);
    } finally {
      busy = false;
    }
  }
  async function open(note: Note) {
    if (note.conflicts.length) {
      conflict = note;
      return;
    }
    if (native) await call('open_note', { noteId: note.id });
    else previewNote = note.id;
  }
  async function create() {
    await task(async () => {
      const note = await saveNote(null, emptyContent());
      await refresh();
      await open(note);
    });
  }
  async function change(note: Note, action: 'archive' | 'delete' | 'restore') {
    await task(async () => {
      const content = { ...note.content };
      if (action === 'archive') content.archived = true;
      if (action === 'delete') content.deleted = true;
      if (action === 'restore') {
        content.deleted = false;
        content.archived = false;
      }
      await saveNote(note, content);
      await refresh();
    });
  }
  async function sync() {
    try {
      status = await call('sync_now');
    } catch (e) {
      notice = String(e);
    }
  }
  async function backup(plainText = false) {
    await task(async () => {
      const path = await call<string | null>('export_notes', { plainText });
      if (path) notice = `已导出到 ${path}`;
    });
  }
  async function restore() {
    await task(async () => {
      const count = await call<number | null>('import_notes');
      if (count !== null) {
        notice = `已合并导入 ${count} 个版本，原有便签保留`;
        await refresh();
      }
    });
  }
  async function quit() {
    if (quitting) return;
    quitting = true;
    try {
      const ids = new Set(await call<string[]>('editor_ids'));
      if (ids.size) {
        const token = crypto.randomUUID();
        await new Promise<void>((resolve, reject) => {
          let off = () => {};
          const timeout = setTimeout(() => {
            off();
            reject(new Error('便签窗口未能确认保存，请检查后再退出'));
          }, 8000);
          void on('editor-flushed', (payload) => {
            const ack = payload as { token: string; noteId: string; ok: boolean };
            if (ack.token !== token) return;
            if (!ack.ok) {
              clearTimeout(timeout);
              off();
              reject(new Error('有便签尚未保存，请处理保存错误后退出'));
              return;
            }
            ids.delete(ack.noteId);
            if (!ids.size) {
              clearTimeout(timeout);
              off();
              resolve();
            }
          })
            .then((unlisten) => {
              off = unlisten;
              return broadcast('flush-editors', token);
            })
            .catch(reject);
        });
      }
      await call('quit_app');
    } catch (e) {
      notice = String(e);
      if (native) await call('open_library');
    } finally {
      quitting = false;
    }
  }
  onMount(() => {
    let disposed = false;
    const off: (() => void)[] = [];
    void refresh().catch((e) => (notice = String(e)));
    for (const event of ['notes-changed', 'sync-changed']) {
      void on(event, () => {
        void refresh().catch((e) => (notice = String(e)));
      }).then((f) => (disposed ? f() : off.push(f)));
    }
    void on('request-close', () => {
      void call('close_note').catch((e) => (notice = String(e)));
    }).then((f) => (disposed ? f() : off.push(f)));
    void on('request-quit', () => {
      void quit();
    }).then((f) => (disposed ? f() : off.push(f)));
    return () => {
      disposed = true;
      off.forEach((f) => f());
    };
  });
</script>

<svelte:window
  onkeydown={(event) => {
    if (settingsOpen || previewNote || conflict) return;
    if ((event.ctrlKey || event.metaKey) && event.key === 'n') {
      event.preventDefault();
      void create();
    }
    if ((event.ctrlKey || event.metaKey) && event.key === 'f') {
      event.preventDefault();
      searchInput?.focus();
    }
  }}
  onfocus={() => {
    if (native) void sync();
  }}
/>

<div class="library-shell">
  <aside class="sidebar">
    <div class="brand">
      <span class="brand-icon"><StickyNote size={23} strokeWidth={1.6} /></span>
      <div><strong>轻笺</strong><small>QINGNOTE</small></div>
    </div>
    <button class="new-button" onclick={create} disabled={busy}
      ><Plus size={18} />新建便签<span>⌘ / Ctrl N</span></button
    >
    <p class="nav-caption">你的空间</p>
    <nav aria-label="便签分类">
      <button class:chosen={filter === 'active'} onclick={() => (filter = 'active')}
        ><StickyNote size={17} />我的便签<span>{count}</span></button
      >
      <button class:chosen={filter === 'archived'} onclick={() => (filter = 'archived')}
        ><Archive size={17} />归档<span>{notes.filter((n) => belongs(n, 'archived')).length}</span
        ></button
      >
      <button class:chosen={filter === 'deleted'} onclick={() => (filter = 'deleted')}
        ><Trash2 size={17} />回收站<span>{notes.filter((n) => belongs(n, 'deleted')).length}</span
        ></button
      >
      {#if conflictCount}<button
          class:chosen={filter === 'conflicts'}
          onclick={() => (filter = 'conflicts')}
          ><AlertTriangle size={17} />待处理冲突<span>{conflictCount}</span></button
        >{/if}
    </nav>
    <div class="sidebar-bottom">
      <div class="sync-card">
        <Cloud size={18} />
        <div>
          <strong
            >{status.running
              ? '正在同步'
              : status.lastSuccess
                ? '已连接云端'
                : '安心记在本机'}</strong
          >
          <p>
            {status.lastSuccess
              ? `上次同步 ${new Date(status.lastSuccess).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
              : '连接 WebDAV，让便签随你走。'}
          </p>
        </div>
      </div>
      <button class="sidebar-settings" onclick={() => (settingsOpen = true)}
        ><Settings size={17} />设置与同步</button
      >
      {#if native}<button class="sidebar-settings subtle" onclick={quit}
          ><LogOut size={16} />退出轻笺</button
        >{/if}
      <span class="version">QingNote · 0.1.1</span>
    </div>
  </aside>

  <main class="library-main">
    <div class="topbar">
      <span class="breadcrumb">个人空间 <span>/</span> {labels[filter]}</span><button
        class="icon-button"
        aria-label="立即同步"
        title="立即同步"
        onclick={sync}
        disabled={!native || status.running}
        ><RefreshCw size={17} class={status.running ? 'spinning' : ''} /></button
      >
    </div>
    {#if !native}<div class="preview-banner">
        浏览器预览 · 便签仅保存在此浏览器；同步与独立窗口请使用桌面版。
      </div>{/if}
    <header class="page-heading">
      <div>
        <div class="eyebrow">A LITTLE SPACE FOR YOUR THOUGHTS</div>
        <h1>{labels[filter]}<span>{visible.length}</span></h1>
        <p>{descriptions[filter]}</p>
      </div>
      <span class="paper-mark" aria-hidden="true"><StickyNote size={40} strokeWidth={1} /></span>
    </header>
    <div class="toolbar">
      <label class="search"
        ><Search size={17} /><input
          bind:this={searchInput!}
          bind:value={query}
          placeholder="搜索便签…"
          aria-label="搜索便签"
        />{#if query}<button class="icon-button" aria-label="清空搜索" onclick={() => (query = '')}
            ><X size={14} /></button
          >{/if}</label
      ><span class="sort-label">最近修改</span>
    </div>
    {#if notice}<div class="notice" role="status">
        <span>{notice}</span><button
          class="icon-button"
          aria-label="关闭提示"
          onclick={() => (notice = '')}><X size={15} /></button
        >
      </div>{/if}
    {#if visible.length}
      <div class="note-grid">
        {#each visible as note (note.id)}
          <article class="note-card note-{note.content.color}">
            <button
              class="card-content"
              aria-label={`打开便签：${title(note)}`}
              onclick={() => task(() => open(note))}
            >
              <h2>{title(note)}</h2>
              <p>
                {(note.content.title?.trim()
                  ? note.content.text
                  : note.content.text.trim().split('\n').slice(1).join('\n')) ||
                  (note.content.text ? '' : '一点想法，就从这里开始。')}
              </p>
            </button>
            {#if note.conflicts.length}<button
                class="conflict-badge"
                onclick={() => (conflict = note)}
                ><AlertTriangle size={13} />有 {note.conflicts.length + 1} 个版本</button
              >{/if}
            <footer>
              <span
                >{shortDate(note.updatedAt)}<i
                  title={note.pending ? '本机有待同步修改' : '已上传'}
                  class:pending={note.pending}
                ></i></span
              >
              <div class="card-actions">
                {#if !note.conflicts.length}
                  {#if filter === 'deleted' || filter === 'archived'}<button
                      class="icon-button"
                      title="恢复便签"
                      aria-label={`恢复 ${title(note)}`}
                      onclick={() => change(note, 'restore')}><RotateCcw size={15} /></button
                    >
                  {:else}<button
                      class="icon-button"
                      title="归档便签"
                      aria-label={`归档 ${title(note)}`}
                      onclick={() => change(note, 'archive')}><Archive size={15} /></button
                    >{/if}
                  {#if !note.content.deleted}<button
                      class="icon-button"
                      title="移到回收站"
                      aria-label={`删除 ${title(note)}`}
                      onclick={() => change(note, 'delete')}><Trash2 size={15} /></button
                    >{/if}
                {/if}
                <button
                  class="icon-button"
                  title="打开便签"
                  aria-label={`展开 ${title(note)}`}
                  onclick={() => task(() => open(note))}><ArrowUpRight size={17} /></button
                >
              </div>
            </footer>
          </article>
        {/each}
        {#if filter === 'active' && !query}<button class="add-card" onclick={create} disabled={busy}
            ><Plus size={22} strokeWidth={1.5} /><span>再记一张</span></button
          >{/if}
      </div>
    {:else}
      <div class="empty-state">
        <div class="empty-paper"><StickyNote size={42} strokeWidth={1.2} /></div>
        <h2>
          {query
            ? '没有找到这张便签'
            : filter === 'active'
              ? '给小想法，留个位置'
              : '这里暂时没有便签'}
        </h2>
        <p>
          {query
            ? '换个关键词试试。'
            : filter === 'active'
              ? '一句提醒、一个灵感，或者今天要做的小事。'
              : '你的便签会在需要时出现在这里。'}
        </p>
        {#if filter === 'active' && !query}<button class="primary" onclick={create}
            ><Plus size={17} />写下第一张便签</button
          >{/if}
      </div>
    {/if}
    <footer class="library-footer">
      <span><span class="status-dot"></span>{status.message}</span>
      <div>
        <button onclick={() => backup(false)} disabled={!native || busy} title="导出完整备份"
          ><Download size={14} />导出</button
        ><button onclick={restore} disabled={!native || busy} title="合并导入轻笺备份"
          ><Upload size={14} />导入</button
        >
      </div>
    </footer>
  </main>
</div>
{#if settingsOpen}<SettingsDialog
    onclose={() => (settingsOpen = false)}
    onchanged={refresh}
    onexport={() => backup(true)}
  />{/if}
{#if conflict}<ConflictDialog
    note={conflict}
    onclose={() => {
      conflict = null;
      void refresh();
    }}
  />{/if}
{#if previewNote}<div class="preview-editor">
    <Editor
      noteId={previewNote}
      onclose={() => {
        previewNote = null;
        void refresh();
      }}
    />
  </div>{/if}
