import { marked, Marked, Renderer } from 'marked';
import markedKatex from 'marked-katex-extension';
import sanitizeHtml from 'sanitize-html';

marked.setOptions({ gfm: true, breaks: true });
marked.use(markedKatex({ throwOnError: false, output: 'htmlAndMathml' }));

export type MarkdownHeading = { id: string; text: string; level: 2 | 3 };

function normalizeTyporaListIndentation(source: string): string {
  let fenced = false;
  return source.split('\n').map((line) => {
    if (/^\s*(`{3,}|~{3,})/.test(line)) {
      fenced = !fenced;
      return line;
    }
    if (fenced) return line;
    const match = line.match(/^( +)(?=(?:[*+-]|\d+[.)])\s+)/);
    if (!match) return line;
    const width = match[1].length;
    if (width < 3) return `${' '.repeat(Math.ceil(width / 2) * 3)}${line.slice(width)}`;
    if (width % 2 === 0) return `${' '.repeat((width / 2) * 3)}${line.slice(width)}`;
    return line;
  }).join('\n');
}

export function extractMarkdownHeadings(source: string): MarkdownHeading[] {
  let headingIndex = 0;
  return marked.lexer(source).flatMap((token) => {
    if (token.type !== 'heading') return [];
    const id = `article-section-${++headingIndex}`;
    if (token.depth !== 2 && token.depth !== 3) return [];
    return [{ id, text: token.text.trim(), level: token.depth }];
  });
}

const mathTags = [
  'math', 'semantics', 'annotation', 'mrow', 'mi', 'mn', 'mo', 'mtext', 'mspace',
  'msup', 'msub', 'msubsup', 'mfrac', 'msqrt', 'mroot', 'mover', 'munder',
  'munderover', 'mtable', 'mtr', 'mtd', 'mpadded', 'mphantom', 'menclose',
  'svg', 'path'
];

export function renderMarkdown(source: string): string {
  let headingIndex = 0;
  const renderer = new Renderer();
  renderer.heading = function ({ tokens, depth }) {
    const id = `article-section-${++headingIndex}`;
    return `<h${depth} id="${id}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
  };
  const markdown = new Marked();
  markdown.setOptions({ gfm: true, breaks: true, renderer });
  markdown.use(markedKatex({ throwOnError: false, output: 'htmlAndMathml' }));
  const html = markdown.parse(normalizeTyporaListIndentation(source)) as string;
  return sanitizeHtml(html, {
    allowedTags: sanitizeHtml.defaults.allowedTags.concat(['h1', 'h2', 'h3', 'h4', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'img', ...mathTags]),
    allowedAttributes: {
      ...sanitizeHtml.defaults.allowedAttributes,
      a: ['href', 'name', 'target', 'rel'],
      h1: ['id'],
      h2: ['id'],
      h3: ['id'],
      h4: ['id'],
      ol: ['start', 'reversed', 'type'],
      li: ['value'],
      img: ['src', 'alt', 'title', 'width', 'height', 'loading'],
      span: ['class', 'style', 'aria-hidden'],
      math: ['xmlns', 'display'],
      annotation: ['encoding'],
      svg: ['xmlns', 'width', 'height', 'viewBox', 'preserveAspectRatio'],
      path: ['d']
    },
    allowedStyles: {
      span: {
        height: [/^-?[\d.]+(?:em|ex|px|%)?$/],
        width: [/^-?[\d.]+(?:em|ex|px|%)?$/],
        top: [/^-?[\d.]+(?:em|ex|px|%)?$/],
        left: [/^-?[\d.]+(?:em|ex|px|%)?$/],
        'margin-left': [/^-?[\d.]+(?:em|ex|px|%)?$/],
        'margin-right': [/^-?[\d.]+(?:em|ex|px|%)?$/],
        'vertical-align': [/^-?[\d.]+(?:em|ex|px|%)?$/],
        'border-bottom-width': [/^[\d.]+(?:em|ex|px)?$/]
      }
    },
    allowedSchemes: ['http', 'https', 'mailto'],
    allowedSchemesByTag: { img: ['http', 'https'] }
  });
}
