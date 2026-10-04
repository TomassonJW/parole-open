import type { ReactNode } from 'react'

function cells(line: string): string[] {
  return line.trim().replace(/^\||\|$/g, '').split(/(?<!\\)\|/).map((cell) => cell.trim().replaceAll('\\|', '|'))
}

const quoteEntities = [
  ['&#33;', '!'], ['&#58;', ':'], ['&#46;', '.'], ['&#64;', '@'],
  ['&#91;', '['], ['&#93;', ']'], ['&#40;', '('], ['&#41;', ')'],
  ['&#42;', '*'], ['&#95;', '_'], ['&#96;', '`'], ['&#126;', '~'], ['&#92;', '\\'],
  ['&#124;', '|'], ['&#35;', '#'], ['&#43;', '+'], ['&#45;', '-'],
  ['&#123;', '{'], ['&#125;', '}'],
] as const

function decodeEncodedText(content: string): string {
  let visible = content
  for (const [encoded, literal] of quoteEntities) visible = visible.replaceAll(encoded, literal)
  return visible.replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&')
}

/** Format limité au compte rendu produit par Parole : aucune interprétation HTML. */
export function ReportDocument({ content, formatVersion = 0 }: { content: string; formatVersion?: number }) {
  if (formatVersion !== 1) {
    return <div className="report report--full report--legacy">
      <p>Ancien compte rendu affiché en texte brut. Actualise-le pour retrouver sa mise en forme.</p>
      <pre className="report__legacy-text">{content}</pre>
    </div>
  }
  const visible = decodeEncodedText
  const blocks = content.split(/\n\s*\n/).map((block) => block.trim()).filter(Boolean)
  return <div className="report report--full">{blocks.map((block, index): ReactNode => {
    const lines = block.split('\n')
    const title = /^(#{1,3})\s+(.+)$/.exec(block)
    if (title && lines.length === 1) {
      const text = visible(title[2])
      if (title[1].length === 1) return <h3 key={index}>{text}</h3>
      if (title[1].length === 2) return <h4 key={index}>{text}</h4>
      return <h5 key={index}>{text}</h5>
    }
    if (lines.length >= 2 && lines[0].startsWith('|') && /^\|[\s|:-]+\|$/.test(lines[1])) {
      const headers = cells(lines[0])
      return <div key={index} className="report__table-wrap"><table><thead><tr>{headers.map((h, n) => <th key={n} scope="col">{visible(h)}</th>)}</tr></thead><tbody>{lines.slice(2).map((line, row) => <tr key={row}>{cells(line).map((cell, col) => <td key={col}>{visible(cell)}</td>)}</tr>)}</tbody></table></div>
    }
    if (lines.every((line) => line.startsWith('- '))) {
      return <ul key={index}>{lines.map((line, n) => <li key={n}>{visible(line.slice(2))}</li>)}</ul>
    }
    if (lines.every((line) => line.startsWith('> '))) {
      return <blockquote key={index}>{lines.map((line, n) => <p key={n}>{visible(line.slice(2))}</p>)}</blockquote>
    }
    const text = block.startsWith('_') && block.endsWith('_') ? block.slice(1, -1) : block
    return <p key={index}>{visible(text)}</p>
  })}</div>
}
