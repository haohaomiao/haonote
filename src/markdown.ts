import { Marked } from 'marked';
import DOMPurify from 'dompurify';

const escape = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const markdown = new Marked({
  gfm: true,
  breaks: true,
  renderer: {
    // Underline is the only supported HTML extension. No attributes are accepted.
    html: ({ text }) => (/^<\/?u>$/.test(text) ? text : escape(text)),
    image: ({ text }) => `[图片：${escape(text)}]`,
    link({ href, tokens }) {
      return `<a title="${escape(href)}">${this.parser.parseInline(tokens)}</a>`;
    },
  },
});

// Notes can come from another device. Only render document formatting, never active HTML.
// Links are readable but inert; images and all network-loading attributes are excluded.
export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(markdown.parse(text, { async: false }), {
    ALLOWED_TAGS: [
      'p',
      'br',
      'h1',
      'h2',
      'h3',
      'h4',
      'h5',
      'h6',
      'strong',
      'em',
      'u',
      'del',
      'blockquote',
      'ul',
      'ol',
      'li',
      'pre',
      'code',
      'hr',
      'table',
      'thead',
      'tbody',
      'tr',
      'th',
      'td',
      'a',
      'input',
    ],
    ALLOWED_ATTR: ['type', 'checked', 'disabled', 'start', 'title'],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  });
}
