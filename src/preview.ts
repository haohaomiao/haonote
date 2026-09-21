// Deliberately small UI preview adapter. Rust tests exercise the real SQLite/WebDAV implementation.
import { emptyContent, type Content, type Note, type Revision } from './types';
const key = 'qingnote-development-preview';
let notes: Note[] = JSON.parse(localStorage.getItem(key) || '[]');
let history: Revision[] = JSON.parse(localStorage.getItem(`${key}-history`) || '[]');
function checkpoint(note: Note) {
  if (history.some((r) => r.id === note.headId)) return;
  history.unshift({
    schema: 1,
    id: note.headId,
    noteId: note.id,
    parents: [],
    content: structuredClone(note.content),
    createdAt: note.createdAt,
    updatedAt: note.updatedAt,
  });
  localStorage.setItem(`${key}-history`, JSON.stringify(history));
}
function changed() {
  localStorage.setItem(key, JSON.stringify(notes));
  window.dispatchEvent(new CustomEvent('notes-changed'));
}
export async function invokePreview(
  command: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  if (command === 'list_notes') return structuredClone(notes);
  if (command === 'note_history') {
    const note = notes.find((n) => n.id === args.noteId);
    if (!note) throw new Error('便签不存在');
    checkpoint(note);
    const offset = Number(args.offset ?? 0);
    return structuredClone(history.filter((r) => r.noteId === note.id).slice(offset, offset + 50));
  }
  if (command === 'restore_revision') {
    const note = notes.find((n) => n.id === args.noteId);
    const revision = history.find((r) => r.noteId === args.noteId && r.id === args.revisionId);
    if (!note || note.headId !== args.expected || note.conflicts.length)
      throw new Error('便签已变化或存在冲突，请重新打开历史并先处理冲突');
    if (!revision) throw new Error('历史版本不存在');
    return invokePreview('save_note', {
      noteId: note.id,
      expected: note.headId,
      content: structuredClone(revision.content),
    });
  }
  if (command === 'save_note') {
    const existing = notes.find((n) => n.id === args.noteId);
    if (existing && existing.headId !== args.expected) throw new Error('便签已在其他窗口更新');
    if (existing) checkpoint(existing);
    const note: Note = {
      id: existing?.id ?? crypto.randomUUID(),
      headId: crypto.randomUUID(),
      content: (args.content as Content) ?? emptyContent(),
      createdAt: existing?.createdAt ?? new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      conflicts: [],
      pending: true,
    };
    notes = [note, ...notes.filter((n) => n.id !== note.id)];
    changed();
    return structuredClone(note);
  }
  if (command === 'get_settings')
    return {
      config: null,
      hasPassword: false,
      autostart: false,
      dataDirectory: '浏览器预览，不是桌面数据库',
    };
  if (command === 'sync_status' || command === 'sync_now')
    return {
      running: false,
      lastSuccess: null,
      message: '浏览器预览 · 仅保存在此浏览器',
      pending: notes.length,
    };
  throw new Error('此功能请在桌面安装版中使用');
}
