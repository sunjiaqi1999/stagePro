import hljs from 'highlight.js/lib/core'
import bash from 'highlight.js/lib/languages/bash'
import css from 'highlight.js/lib/languages/css'
import diff from 'highlight.js/lib/languages/diff'
import javascript from 'highlight.js/lib/languages/javascript'
import json from 'highlight.js/lib/languages/json'
import markdown from 'highlight.js/lib/languages/markdown'
import rust from 'highlight.js/lib/languages/rust'
import shell from 'highlight.js/lib/languages/shell'
import typescript from 'highlight.js/lib/languages/typescript'
import xml from 'highlight.js/lib/languages/xml'
import 'highlight.js/styles/github.css'
import MarkdownIt from 'markdown-it'

hljs.registerLanguage('bash', bash)
hljs.registerLanguage('css', css)
hljs.registerLanguage('diff', diff)
hljs.registerLanguage('javascript', javascript)
hljs.registerLanguage('js', javascript)
hljs.registerLanguage('json', json)
hljs.registerLanguage('markdown', markdown)
hljs.registerLanguage('md', markdown)
hljs.registerLanguage('rust', rust)
hljs.registerLanguage('rs', rust)
hljs.registerLanguage('shell', shell)
hljs.registerLanguage('sh', shell)
hljs.registerLanguage('typescript', typescript)
hljs.registerLanguage('ts', typescript)
hljs.registerLanguage('html', xml)
hljs.registerLanguage('vue', xml)

function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

const markdownParser = new MarkdownIt({
  breaks: true,
  html: false,
  linkify: true,
  typographer: true,
  highlight(code, language) {
    const normalizedLanguage = language?.trim()

    if (normalizedLanguage && hljs.getLanguage(normalizedLanguage)) {
      try {
        return hljs.highlight(code, { language: normalizedLanguage, ignoreIllegals: true }).value
      } catch {
        return escapeHtml(code)
      }
    }

    try {
      return hljs.highlightAuto(code).value
    } catch {
      return escapeHtml(code)
    }
  },
})

const defaultFenceRender = markdownParser.renderer.rules.fence
const defaultTableOpenRender = markdownParser.renderer.rules.table_open
const defaultTableCloseRender = markdownParser.renderer.rules.table_close

markdownParser.renderer.rules.fence = (tokens, index, options, env, self) => {
  const token = tokens[index]
  const language = token.info.trim()
  const label = (language || 'text').toUpperCase()
  const rendered = defaultFenceRender
    ? defaultFenceRender(tokens, index, options, env, self)
    : `<pre><code>${escapeHtml(token.content)}</code></pre>`

  return `<div class="ai-md-code-block"><div class="ai-md-code-label">${escapeHtml(label)}</div>${rendered}</div>`
}

markdownParser.renderer.rules.table_open = (tokens, index, options, env, self) => {
  const tableOpenHtml = defaultTableOpenRender ? defaultTableOpenRender(tokens, index, options, env, self) : '<table>'
  return `<div class="ai-md-table-wrap">${tableOpenHtml}`
}

markdownParser.renderer.rules.table_close = (tokens, index, options, env, self) => {
  const tableCloseHtml = defaultTableCloseRender ? defaultTableCloseRender(tokens, index, options, env, self) : '</table>'
  return `${tableCloseHtml}</div>`
}

const markdownBreakTagTestPattern = /(?:<br\s*\/?>|&lt;br\s*\/?&gt;)/i
const markdownBreakTagReplacePattern = /(?:<br\s*\/?>|&lt;br\s*\/?&gt;)/gi

function normalizeMarkdownBreakTags(markdown: string) {
  let inFence = false

  return markdown
    .split('\n')
    .map((line) => {
      if (/^\s*```/.test(line)) {
        inFence = !inFence
        return line
      }

      if (inFence || !markdownBreakTagTestPattern.test(line)) return line

      const replacement = line.trim().startsWith('|') ? '；' : '\n'
      return line.replace(markdownBreakTagReplacePattern, replacement)
    })
    .join('\n')
}

export function renderMarkdownToHtml(markdown: string) {
  const normalizedMarkdown = normalizeMarkdownBreakTags(markdown.trim())
  if (!normalizedMarkdown) return '<div class="ai-md-root"></div>'
  return `<div class="ai-md-root">${markdownParser.render(normalizedMarkdown)}</div>`
}
