import { StarterKit } from '@tiptap/starter-kit';
import { Underline } from '@tiptap/extension-underline';
import { Markdown } from '@tiptap/markdown';
import { TableKit } from '@tiptap/extension-table';
import { TaskList } from '@tiptap/extension-task-list';
import { TaskItem } from '@tiptap/extension-task-item';
import { marked } from 'marked';
import { renderMarkdown } from './markdown';

// Keep the existing <u> convention instead of introducing a second underline syntax.
const CompatibleUnderline = Underline.extend({
  renderMarkdown(node, helpers) {
    return `<u>${helpers.renderChildren(node)}</u>`;
  },
  markdownTokenizer: {
    name: 'underline',
    level: 'inline',
    start: (source) => source.indexOf('<u>'),
    tokenize(source, _tokens, lexer) {
      const match = /^<u>([\s\S]+?)<\/u>/.exec(source);
      return match
        ? { type: 'underline', raw: match[0], tokens: lexer.inlineTokens(match[1]) }
        : undefined;
    },
  },
});

export const richExtensions = () => [
  StarterKit.configure({
    underline: false,
    link: { openOnClick: false, autolink: false, linkOnPaste: false },
  }),
  CompatibleUnderline,
  TableKit.configure({ table: { resizable: false } }),
  TaskList,
  TaskItem.configure({ nested: true }),
  Markdown.configure({ markedOptions: { gfm: true, breaks: true } }),
];

// Check before Tiptap parses HTML so excluded content cannot trigger network loads.
export function unsupportedMarkdown(source: string): boolean {
  let unsupported = /^\[\^[^\]]+\]:/m.test(source);
  marked.walkTokens(marked.lexer(source), (token) => {
    if (token.type === 'image') unsupported = true;
    if (token.type === 'html' && !/^<\/?u>$/.test(token.raw)) unsupported = true;
    if (token.type === 'link' && !/^(https?:|mailto:|#|\/)/i.test(token.href)) unsupported = true;
  });
  return unsupported;
}

// Conservative round-trip check. Original bytes are kept until a real edit occurs.
// If the converter changes visible text, structure or formatting, keep a read-only preview.
export function sameMarkdownMeaning(before: string, after: string): boolean {
  const signature = (source: string) => {
    const document = new DOMParser().parseFromString(renderMarkdown(source), 'text/html');
    const root = document.body;
    return JSON.stringify({
      text: root.textContent?.replace(/\s+/g, ' ').trim(),
      blocks: [
        ...root.querySelectorAll('h1,h2,h3,h4,h5,h6,ul,ol,li,blockquote,pre,table,tr,hr'),
      ].map((n) => n.tagName),
      marks: ['strong', 'em', 'u', 'del', 'code'].map((tag) =>
        [...root.querySelectorAll(tag)].map((n) => n.textContent).sort(),
      ),
      links: [...root.querySelectorAll('a')].map((n) => n.title),
      checks: [...root.querySelectorAll('input')].map((n) => n.checked),
    });
  };
  return signature(before) === signature(after);
}
