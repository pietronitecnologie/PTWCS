// Renderer Markdown minimale (titoli con ancore, paragrafi, elenchi annidati,
// tabelle, blocchi di codice, citazioni, enfasi, codice, link, immagini) e aiuto
// contestuale del manuale.
import { esc } from './core.js';

export const MANUAL_PAGES = [
  ['index', 'Introduzione'], ['runtime', 'Runtime'], ['wcs', 'WCS magazzini'],
  ['designer', 'Designer – Viste'], ['designer-tag', 'Designer – Tag e UDT'], ['designer-smistamento', 'Designer – Smistamento'],
  ['designer-progetto', 'Designer – Progetto'], ['designer-script', 'Designer – Script'],
];

/** Ancora di un titolo (come GitHub: minuscolo, senza accenti e punteggiatura, spazi → -). */
export function slug(text) {
  return text.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[`*_]/g, '')
    .replace(/[^\p{L}\p{N}\s-]/gu, '').trim().replace(/\s/g, '-');
}

function inline(s, link) {
  const codes = [];
  s = s.replace(/`([^`]+)`/g, (_, c) => { codes.push(c); return `\u0000${codes.length - 1}\u0000`; });
  s = esc(s)
    .replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, a, u) => `<img src="${link(u, true)}" alt="${a}">`)
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, t, u) => `<a href="${link(u)}"${/^https?:/.test(u) ? ' target="_blank" rel="noopener"' : ''}>${t}</a>`)
    .replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
    .replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, '$1<i>$2</i>')
    .replace(/(^|[\s(])_([^_\s][^_]*)_(?=[\s).,;:]|$)/g, '$1<i>$2</i>');
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${esc(codes[+i])}</code>`);
}

/**
 * Converte Markdown in HTML. `link(url, isImage)` riscrive i link relativi.
 * Ritorna { html, toc: [{level, text, id}] }.
 */
export function renderMarkdown(md, { link = (u) => u } = {}) {
  const lines = md.replace(/\r/g, '').split('\n');
  const out = [], toc = [];
  let i = 0;
  const isTableSep = (l) => /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(l);
  const cells = (l) => l.trim().replace(/^\|/, '').replace(/\|$/, '').split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, '|'));
  while (i < lines.length) {
    const l = lines[i];
    if (!l.trim()) { i++; continue; }
    let m;
    if ((m = l.match(/^```(\w*)/))) {
      const buf = [];
      i++;
      while (i < lines.length && !lines[i].startsWith('```')) buf.push(lines[i++]);
      i++;
      out.push(`<pre class="md-code${m[1] ? ' lang-' + m[1] : ''}"><code>${esc(buf.join('\n'))}</code></pre>`);
      continue;
    }
    if ((m = l.match(/^(#{1,6})\s+(.*)$/))) {
      const level = m[1].length, text = m[2].trim(), id = slug(text);
      toc.push({ level, text: text.replace(/[`*]/g, ''), id });
      out.push(`<h${level} id="${id}">${inline(text, link)}<a class="md-anchor" href="#${id}" data-anchor="${id}">#</a></h${level}>`);
      i++;
      continue;
    }
    if (l.includes('|') && i + 1 < lines.length && isTableSep(lines[i + 1])) {
      const head = cells(l);
      i += 2;
      const rows = [];
      while (i < lines.length && lines[i].includes('|') && lines[i].trim()) rows.push(cells(lines[i++]));
      out.push(`<div class="md-tbl"><table><thead><tr>${head.map((h) => `<th>${inline(h, link)}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c) => `<td>${inline(c, link)}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`);
      continue;
    }
    if (l.startsWith('>')) {
      const buf = [];
      while (i < lines.length && lines[i].startsWith('>')) buf.push(lines[i++].replace(/^>\s?/, ''));
      out.push(`<blockquote>${renderMarkdown(buf.join('\n'), { link }).html}</blockquote>`);
      continue;
    }
    if (/^\s*([-*]|\d+\.)\s+/.test(l)) {
      // elenco (anche annidato per rientro)
      const items = [];
      while (i < lines.length && (/^\s*([-*]|\d+\.)\s+/.test(lines[i]) || (/^\s{2,}\S/.test(lines[i]) && items.length))) {
        const x = lines[i].match(/^(\s*)([-*]|\d+\.)\s+(.*)$/);
        if (x) items.push({ ind: x[1].length, ord: /\d/.test(x[2]), text: x[3] });
        else items[items.length - 1].text += ' ' + lines[i].trim();
        i++;
      }
      const build = (from, ind) => {
        let html = '', k = from;
        const ord = items[from].ord;
        html += ord ? '<ol>' : '<ul>';
        while (k < items.length && items[k].ind >= ind) {
          if (items[k].ind > ind) { const [h, n] = build(k, items[k].ind); html = html.replace(/<\/li>$/, h + '</li>'); k = n; continue; }
          html += `<li>${inline(items[k].text, link)}</li>`;
          k++;
        }
        return [html + (ord ? '</ol>' : '</ul>'), k];
      };
      out.push(build(0, items[0].ind)[0]);
      continue;
    }
    const buf = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|```|>|\s*([-*]|\d+\.)\s+)/.test(lines[i]) && !(lines[i].includes('|') && i + 1 < lines.length && isTableSep(lines[i + 1]))) buf.push(lines[i++].trim());
    out.push(`<p>${inline(buf.join(' '), link)}</p>`);
  }
  return { html: out.join('\n'), toc };
}

/** Link interni del manuale: `pagina.md#ancora` → `manual.html#pagina/ancora`. */
export function manualLink(base = 'manual.html') {
  return (u, img) => {
    if (img || /^(https?:|mailto:)/.test(u)) return u;
    const [p, a] = u.split('#');
    if (!p) return `${base}#${currentPage()}/${a}`;
    return `${base}#${p.replace(/\.md$/, '')}${a ? '/' + a : ''}`;
  };
}
let _cur = 'index';
export const currentPage = () => _cur;
export const setCurrentPage = (p) => (_cur = p);

const cache = {};
export async function loadPage(page) {
  if (!cache[page]) {
    const r = await fetch(`manual/${page}.md`);
    if (!r.ok) throw new Error(`pagina ${page} non trovata`);
    cache[page] = await r.text();
  }
  return cache[page];
}

/** Estrae la sezione che inizia con il titolo `anchor` fino al titolo successivo di pari o superiore livello. */
export function sectionOf(md, anchor) {
  const lines = md.split('\n');
  const start = lines.findIndex((l) => { const m = l.match(/^(#{1,6})\s+(.*)$/); return m && slug(m[2]) === anchor; });
  if (start < 0) return null;
  const level = lines[start].match(/^#+/)[0].length;
  let end = lines.length;
  let fence = false;
  for (let k = start + 1; k < lines.length; k++) {
    if (lines[k].startsWith('```')) fence = !fence;
    const m = !fence && lines[k].match(/^(#{1,6})\s/);
    if (m && m[1].length <= level) { end = k; break; }
  }
  return lines.slice(start, end).join('\n');
}

// ---------------------------------------------------------------- aiuto contestuale

/** Pulsante "?" che apre la sezione del manuale: `pagina#ancora`. */
export const helpBtn = (ref, title = 'Guida') => `<button type="button" class="helpbtn" data-help="${esc(ref)}" title="${esc(title)} – apri la guida">?</button>`;

let drawer = null;
export async function openHelp(ref) {
  const [page, anchor] = ref.split('#');
  if (!drawer) {
    drawer = document.createElement('aside');
    drawer.className = 'help-drawer';
    document.body.appendChild(drawer);
  }
  drawer.innerHTML = '<div class="hd-head"><b>📖 Guida</b><span class="grow"></span><a class="hd-open" target="_blank">Apri nel manuale ↗</a><button class="small hd-close">✕</button></div><div class="hd-body md">Caricamento…</div>';
  drawer.classList.add('open');
  drawer.querySelector('.hd-close').onclick = () => drawer.classList.remove('open');
  drawer.querySelector('.hd-open').href = `manual.html#${page}${anchor ? '/' + anchor : ''}`;
  const body = drawer.querySelector('.hd-body');
  try {
    const md = await loadPage(page);
    setCurrentPage(page);
    const sec = (anchor && sectionOf(md, anchor)) || md;
    body.innerHTML = renderMarkdown(sec, { link: manualLink() }).html;
    // i link interni alla stessa guida restano nel pannello
    body.querySelectorAll('a[href^="manual.html#"]').forEach((a) => (a.onclick = (e) => {
      e.preventDefault();
      const [p, an] = a.getAttribute('href').slice('manual.html#'.length).split('/');
      openHelp(`${p}${an ? '#' + an : ''}`);
    }));
    body.querySelectorAll('[data-anchor]').forEach((a) => a.remove());
    body.scrollTop = 0;
  } catch (e) {
    body.textContent = e.message;
  }
}

document.addEventListener('click', (e) => {
  const b = e.target.closest('[data-help]');
  if (b) { e.preventDefault(); e.stopPropagation(); openHelp(b.dataset.help); }
}, true);
document.addEventListener('keydown', (e) => { if (e.key === 'Escape') drawer?.classList.remove('open'); if (e.key === 'F1' && drawer) { /* F1 gestito dalle pagine */ } });
