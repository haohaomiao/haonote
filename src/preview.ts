// Deliberately small UI preview adapter. Rust tests exercise the real SQLite/WebDAV implementation.
import { emptyContent, type Content, type Note } from './types';
const key = 'qingnote-development-preview';
let notes: Note[] = JSON.parse(localStorage.getItem(key) || '[]');
function changed() {
  localStorage.setItem(key, JSON.stringify(notes));
  window.dispatchEvent(new CustomEvent('notes-changed'));
}
export async function invokePreview(
  command: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  if (command === 'list_notes') return structuredClone(notes);
  if (command === 'save_note') {
    const existing = notes.find((n) => n.id === args.noteId);
    if (existing && existing.headId !== args.expected) throw new Error('便签已在其他窗口更新');
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
