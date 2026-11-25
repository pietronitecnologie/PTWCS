// Renderer delle viste (condiviso tra Runtime e Designer).
//
// Modello vista:
// { id, title, width, height, background, home, area?, params: {nome: default}, components: [Component] }
// Component:
// { id, type, name, x, y, w, h, rotation,
//   custom:   { nome: valore }  proprietà personalizzate (possono usare {param} della vista)
//   props:    { ...proprietà statiche del tipo, visible, opacity, tooltip },
//   bindings: { "<path prop>": Binding },          // es. "running", "elements.body.fill"
//   events:   { onClick: { confirm?, actions: [Action] } } }
// Binding (i {nome} sono risolti con custom del componente e parametri della vista):
// { kind: "tag", tag: "{device}.Run", transform? }  |  { kind: "expr", expr: "tag('{device}.Level') > 3", transform? }
// Nelle espressioni: tag(nome), param(nome), value, Math.
// Transform:
// { kind: "map", rules: [{ in: "true" | "10..20" | ">5", out: "..." }], default }
// { kind: "expr", expr: "value / 10" }   |   { kind: "format", format: "{value} kg", decimals: 1 }
// Action:
// { kind: "write", tag, value }  (value: valore | "toggle" | "prompt")
// { kind: "script", fn, payload }  |  { kind: "navigate", view }
// { kind: "popup", view, params: { nome: "valore o {param}" }, title }   (faceplate)

import { esc } from './core.js';

const C = {
  run: '#22c55e', stop: '#64748b', fault: '#ef4444', warn: '#f59e0b',
  line: '#94a3b8', text: '#e2e8f0', panel: '#1e293b',
};

const num = (v, d = 0) => (v === null || v === undefined || v === '' || isNaN(+v) ? d : +v);
const bool = (v) => v === true || v === 1 || v === 'true' || v === '1' || v === 'on';

function textEl(x, y, s, { size = 14, color = C.text, anchor = 'middle', bold = false } = {}) {
  return `<text x="${x}" y="${y}" fill="${esc(color)}" font-size="${size}" text-anchor="${anchor}"
    dominant-baseline="central" font-weight="${bold ? 700 : 400}">${esc(s)}</text>`;
}

// ------------------------------------------------------------------ componenti
// schema: proprietà editabili nel Designer (tutte bindabili).
export const COMPONENTS = {
  conveyor: {
    label: 'Nastro', icon: '▭', size: [200, 40],
    props: { running: false, fault: false, direction: 'right', label: 'CV', colorRun: C.run, colorStop: C.stop, colorFault: C.fault },
    schema: [
      ['running', 'bool', 'In marcia'], ['fault', 'bool', 'Guasto'],
      ['direction', 'select', 'Direzione', ['right', 'left']], ['label', 'text', 'Etichetta'],
      ['colorRun', 'color', 'Colore marcia'], ['colorStop', 'color', 'Colore fermo'], ['colorFault', 'color', 'Colore guasto'],
    ],
    render(p, w, h) {
      const col = bool(p.fault) ? p.colorFault : bool(p.running) ? p.colorRun : p.colorStop;
      const r = Math.min(h / 2, 14);
      let rollers = '';
      for (let x = r; x < w - r + 1; x += Math.max(2 * r, 24)) rollers += `<circle cx="${x}" cy="${h / 2}" r="${r * 0.35}" fill="#0f172a" opacity=".55"/>`;
      const dir = p.direction === 'left' ? -1 : 1;
      const ax = w / 2, ay = h / 2, s = Math.min(h * 0.3, 10);
      const arrow = `<path d="M${ax - dir * s} ${ay - s} L${ax + dir * s} ${ay} L${ax - dir * s} ${ay + s}" fill="none" stroke="#fff" stroke-width="2.5" opacity=".85"/>`;
      const anim = bool(p.running) && !bool(p.fault)
        ? `<line x1="0" y1="2" x2="${w}" y2="2" stroke="#fff" stroke-width="2" stroke-dasharray="8 10" class="${dir > 0 ? 'flow' : 'flow-rev'}" opacity=".6"/>
           <line x1="0" y1="${h - 2}" x2="${w}" y2="${h - 2}" stroke="#fff" stroke-width="2" stroke-dasharray="8 10" class="${dir > 0 ? 'flow' : 'flow-rev'}" opacity=".6"/>` : '';
      return `<rect width="${w}" height="${h}" rx="${r}" fill="${esc(col)}" stroke="#0f172a" stroke-width="1.5"/>
        ${rollers}${anim}${arrow}
        ${p.label ? textEl(w / 2, -10, p.label, { size: 12 }) : ''}`;
    },
  },
  motor: {
    label: 'Motore', icon: 'Ⓜ', size: [60, 60],
    props: { running: false, fault: false, label: 'M1' },
    schema: [['running', 'bool', 'In marcia'], ['fault', 'bool', 'Guasto'], ['label', 'text', 'Etichetta']],
    render(p, w, h) {
      const col = bool(p.fault) ? C.fault : bool(p.running) ? C.run : C.stop;
      const r = Math.min(w, h) / 2 - 2;
      return `<circle cx="${w / 2}" cy="${h / 2}" r="${r}" fill="${col}" stroke="#0f172a" stroke-width="2"/>
        ${textEl(w / 2, h / 2, 'M', { size: r, bold: true, color: '#fff' })}
        ${p.label ? textEl(w / 2, h + 10, p.label, { size: 12 }) : ''}`;
    },
  },
  lamp: {
    label: 'Lampada', icon: '●', size: [30, 30],
    props: { on: false, colorOn: C.run, colorOff: '#334155', label: '' },
    schema: [['on', 'bool', 'Accesa'], ['colorOn', 'color', 'Colore ON'], ['colorOff', 'color', 'Colore OFF'], ['label', 'text', 'Etichetta']],
    render(p, w, h) {
      const on = bool(p.on), r = Math.min(w, h) / 2 - 2;
      return `<circle cx="${w / 2}" cy="${h / 2}" r="${r}" fill="${esc(on ? p.colorOn : p.colorOff)}" stroke="#0f172a" stroke-width="2"
          ${on ? `style="filter: drop-shadow(0 0 6px ${esc(p.colorOn)})"` : ''}/>
        ${p.label ? textEl(w + 6, h / 2, p.label, { size: 13, anchor: 'start' }) : ''}`;
    },
  },
  value: {
    label: 'Valore', icon: '#', size: [120, 36],
    props: { value: 0, decimals: 0, unit: '', fontSize: 18, color: C.text, background: '#0f172a', label: '' },
    schema: [['value', 'text', 'Valore'], ['decimals', 'number', 'Decimali'], ['unit', 'text', 'Unità'],
      ['fontSize', 'number', 'Dim. font'], ['color', 'color', 'Colore testo'], ['background', 'color', 'Sfondo'], ['label', 'text', 'Etichetta']],
    render(p, w, h) {
      let v = p.value;
      if (v !== null && v !== '' && !isNaN(+v) && typeof v !== 'boolean') v = (+v).toFixed(num(p.decimals));
      return `<rect width="${w}" height="${h}" rx="4" fill="${esc(p.background)}" stroke="#334155"/>
        ${textEl(w - 8, h / 2, `${v ?? '—'}${p.unit ? ' ' + p.unit : ''}`, { size: num(p.fontSize, 18), color: p.color, anchor: 'end' })}
        ${p.label ? textEl(0, -10, p.label, { size: 12, anchor: 'start', color: '#94a3b8' }) : ''}`;
    },
  },
  label: {
    label: 'Testo', icon: 'T', size: [160, 30],
    props: { text: 'Testo', fontSize: 16, color: C.text, align: 'start', bold: false },
    schema: [['text', 'text', 'Testo'], ['fontSize', 'number', 'Dim. font'], ['color', 'color', 'Colore'],
      ['align', 'select', 'Allineamento', ['start', 'middle', 'end']], ['bold', 'bool', 'Grassetto']],
    render(p, w, h) {
      const x = p.align === 'middle' ? w / 2 : p.align === 'end' ? w : 0;
      return `<rect width="${w}" height="${h}" fill="transparent"/>` +
        textEl(x, h / 2, p.text, { size: num(p.fontSize, 16), color: p.color, anchor: p.align, bold: bool(p.bold) });
    },
  },
  button: {
    label: 'Pulsante', icon: '⏻', size: [120, 40],
    props: { text: 'Comando', fill: '#2563eb', color: '#ffffff', fontSize: 15, active: false, activeFill: '#16a34a' },
    schema: [['text', 'text', 'Testo'], ['fill', 'color', 'Colore'], ['color', 'color', 'Colore testo'],
      ['fontSize', 'number', 'Dim. font'], ['active', 'bool', 'Attivo (feedback)'], ['activeFill', 'color', 'Colore attivo']],
    render(p, w, h) {
      return `<rect width="${w}" height="${h}" rx="6" fill="${esc(bool(p.active) ? p.activeFill : p.fill)}" stroke="#0f172a" class="btn"/>
        ${textEl(w / 2, h / 2, p.text, { size: num(p.fontSize, 15), color: p.color, bold: true })}`;
    },
  },
  tank: {
    label: 'Serbatoio', icon: '▯', size: [80, 140],
    props: { level: 50, min: 0, max: 100, fill: '#3b82f6', label: '' },
    schema: [['level', 'number', 'Livello'], ['min', 'number', 'Min'], ['max', 'number', 'Max'], ['fill', 'color', 'Colore'], ['label', 'text', 'Etichetta']],
    render(p, w, h) {
      const f = Math.max(0, Math.min(1, (num(p.level) - num(p.min)) / ((num(p.max, 100) - num(p.min)) || 1)));
      return `<rect width="${w}" height="${h}" rx="10" fill="#0f172a" stroke="${C.line}" stroke-width="2"/>
        <rect x="3" y="${3 + (h - 6) * (1 - f)}" width="${w - 6}" height="${(h - 6) * f}" rx="8" fill="${esc(p.fill)}"/>
        ${textEl(w / 2, h / 2, `${Math.round(f * 100)}%`, { size: 14, bold: true })}
        ${p.label ? textEl(w / 2, h + 12, p.label, { size: 12 }) : ''}`;
    },
  },
  bar: {
    label: 'Barra', icon: '▬', size: [160, 22],
    props: { value: 0, min: 0, max: 100, fill: '#0ea5e9', vertical: false },
    schema: [['value', 'number', 'Valore'], ['min', 'number', 'Min'], ['max', 'number', 'Max'], ['fill', 'color', 'Colore'], ['vertical', 'bool', 'Verticale']],
    render(p, w, h) {
      const f = Math.max(0, Math.min(1, (num(p.value) - num(p.min)) / ((num(p.max, 100) - num(p.min)) || 1)));
      const bar = bool(p.vertical)
        ? `<rect x="0" y="${h * (1 - f)}" width="${w}" height="${h * f}" fill="${esc(p.fill)}"/>`
        : `<rect width="${w * f}" height="${h}" fill="${esc(p.fill)}"/>`;
      return `<rect width="${w}" height="${h}" fill="#0f172a" stroke="#334155"/>${bar}`;
    },
  },
  rect: {
    label: 'Rettangolo', icon: '□', size: [120, 80],
    props: { fill: '#1e293b', stroke: '#475569', strokeWidth: 2, radius: 4, text: '', color: C.text, fontSize: 14 },
    schema: [['fill', 'color', 'Riempimento'], ['stroke', 'color', 'Bordo'], ['strokeWidth', 'number', 'Spessore'],
      ['radius', 'number', 'Raggio'], ['text', 'text', 'Testo'], ['color', 'color', 'Colore testo'], ['fontSize', 'number', 'Dim. font']],
    render(p, w, h) {
      return `<rect width="${w}" height="${h}" rx="${num(p.radius)}" fill="${esc(p.fill)}" stroke="${esc(p.stroke)}" stroke-width="${num(p.strokeWidth)}"/>
        ${p.text ? textEl(w / 2, h / 2, p.text, { size: num(p.fontSize, 14), color: p.color }) : ''}`;
    },
  },
  ellipse: {
    label: 'Ellisse', icon: '○', size: [80, 80],
    props: { fill: '#1e293b', stroke: '#475569', strokeWidth: 2 },
    schema: [['fill', 'color', 'Riempimento'], ['stroke', 'color', 'Bordo'], ['strokeWidth', 'number', 'Spessore']],
    render(p, w, h) {
      return `<ellipse cx="${w / 2}" cy="${h / 2}" rx="${w / 2}" ry="${h / 2}" fill="${esc(p.fill)}" stroke="${esc(p.stroke)}" stroke-width="${num(p.strokeWidth)}"/>`;
    },
  },
  line: {
    label: 'Linea / Tubo', icon: '╱', size: [160, 4],
    props: { stroke: C.line, strokeWidth: 4, dashed: false, flow: false },
    schema: [['stroke', 'color', 'Colore'], ['strokeWidth', 'number', 'Spessore'], ['dashed', 'bool', 'Tratteggiata'], ['flow', 'bool', 'Flusso animato']],
    render(p, w, h) {
      return `<line x1="0" y1="${h / 2}" x2="${w}" y2="${h / 2}" stroke="${esc(p.stroke)}" stroke-width="${num(p.strokeWidth, 4)}"
        ${bool(p.dashed) || bool(p.flow) ? 'stroke-dasharray="10 8"' : ''} class="${bool(p.flow) ? 'flow' : ''}" stroke-linecap="round"/>
        <rect width="${w}" height="${Math.max(h, 10)}" y="${Math.min(0, h / 2 - 5)}" fill="transparent"/>`;
    },
  },
  symbol: {
    label: 'Simbolo SVG', icon: '★', size: [100, 100],
    props: { symbol: '', fill: '', elements: {} },
    schema: [['symbol', 'symbol', 'Simbolo'], ['fill', 'color', 'Colore globale']],
    render(p, w, h, ctx) {
      const s = ctx.symbols?.[p.symbol];
      if (!s) return `<rect width="${w}" height="${h}" fill="#1e293b" stroke="#ef4444" stroke-dasharray="4"/>${textEl(w / 2, h / 2, p.symbol ? `? ${p.symbol}` : 'simbolo', { size: 12 })}`;
      return `<svg width="${w}" height="${h}" viewBox="${esc(s.viewBox)}" preserveAspectRatio="none" overflow="visible"
        ${p.fill ? `fill="${esc(p.fill)}"` : ''}>${s.body.replaceAll('__P__', `${ctx.uid}_`)}</svg>`;
    },
    // Applica le proprietà per elemento (props.elements.<id>.<attr>)
    after(g, p) {
      for (const [id, a] of Object.entries(p.elements || {})) {
        g.querySelectorAll(`[data-el="${CSS.escape(id)}"]`).forEach((el) => applyElement(el, a));
      }
    },
  },
  embed: {
    label: 'Vista incorporata', icon: '▣', size: [300, 200],
    props: { view: '', params: '{}', border: false },
    schema: [['view', 'view', 'Vista'], ['params', 'json', 'Parametri (JSON, {param} del padre)'], ['border', 'bool', 'Bordo']],
    render(p, w, h) {
      return `<rect width="${w}" height="${h}" fill="transparent" ${bool(p.border) ? 'stroke="#334155"' : ''}/>
        <svg class="embed-host" width="${w}" height="${h}" preserveAspectRatio="xMidYMid meet"></svg>`;
    },
  },
  trend: {
    label: 'Trend', icon: '📈', size: [400, 160],
    props: { tags: '', minutes: 10, min: '', max: '', background: '#0a1120', legend: true },
    schema: [['tags', 'text', 'Tag (separati da virgola)'], ['minutes', 'number', 'Finestra (minuti)'],
      ['min', 'text', 'Min (vuoto = auto)'], ['max', 'text', 'Max (vuoto = auto)'], ['background', 'color', 'Sfondo'], ['legend', 'bool', 'Legenda']],
    render(p, w, h, ctx) {
      const tags = String(p.tags || '').split(',').map((t) => t.trim()).filter(Boolean);
      const span = Math.max(0.5, num(p.minutes, 10)) * 60000;
      const now = Date.now(), from = now - span;
      const series = tags.map((t) => ({ t, d: (ctx.history?.(t) || []).filter((x) => x[0] >= from - span) }));
      const vals = series.flatMap((s) => s.d.map((x) => x[1])).filter((x) => typeof x === 'number' && isFinite(x));
      let lo = p.min !== '' && p.min !== undefined ? num(p.min) : Math.min(...vals);
      let hi = p.max !== '' && p.max !== undefined ? num(p.max) : Math.max(...vals);
      if (!isFinite(lo) || !isFinite(hi)) { lo = 0; hi = 1; }
      if (hi === lo) { hi += 1; lo -= 1; }
      const L = 40, R = 6, T = 6, B = 16, pw = w - L - R, ph = h - T - B;
      const X = (ts) => L + pw * (ts - from) / span, Y = (v) => T + ph * (1 - (v - lo) / (hi - lo));
      let out = `<rect width="${w}" height="${h}" rx="4" fill="${esc(p.background)}" stroke="#24324f"/>`;
      for (let i = 0; i <= 4; i++) {
        const y = T + ph * i / 4, v = hi - (hi - lo) * i / 4;
        out += `<line x1="${L}" y1="${y}" x2="${w - R}" y2="${y}" stroke="#1e293b"/><text x="${L - 4}" y="${y}" fill="#64748b" font-size="9" text-anchor="end" dominant-baseline="central">${+v.toFixed(2)}</text>`;
      }
      out += `<text x="${L}" y="${h - 5}" fill="#64748b" font-size="9">-${num(p.minutes, 10)} min</text><text x="${w - R}" y="${h - 5}" fill="#64748b" font-size="9" text-anchor="end">ora</text>`;
      series.forEach((s, k) => {
        const col = TREND_COLORS[k % TREND_COLORS.length];
        const pts = s.d.filter((x) => typeof x[1] === 'number');
        if (pts.length) {
          let d = '';
          const all = [...pts, [now, pts.at(-1)[1]]];
          all.forEach(([ts, v], i) => {
            const x = Math.max(L, X(ts)).toFixed(1), y = Y(v).toFixed(1);
            d += i ? ` H${x} V${y}` : `M${x} ${y}`;
          });
          out += `<path d="${d}" fill="none" stroke="${col}" stroke-width="1.6"/>`;
        }
        if (bool(p.legend)) out += `<text x="${L + 6}" y="${T + 10 + k * 13}" fill="${col}" font-size="10">${esc(s.t)}${pts.length ? ' = ' + (+pts.at(-1)[1].toFixed(2)) : ''}</text>`;
      });
      return out;
    },
  },
};
export const TREND_COLORS = ['#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#a855f7', '#06b6d4', '#ec4899'];

export const COMMON_SCHEMA = [['visible', 'bool', 'Visibile'], ['opacity', 'number', 'Opacità (0-1)'], ['tooltip', 'text', 'Tooltip']];
/** Attributi animabili degli elementi di un simbolo SVG. */
export const ELEMENT_ATTRS = ['fill', 'stroke', 'visible', 'opacity', 'text', 'rotate', 'tx', 'ty', 'flow', 'blink', 'spin'];
export const ELEMENT_ATTR_TYPES = { visible: 'bool', flow: 'bool', blink: 'bool', spin: 'bool', fill: 'color', stroke: 'color', text: 'text' };

function applyElement(el, a) {
  const set = (v) => v !== undefined && v !== '' && v !== null;
  if (set(a.fill)) { el.setAttribute('fill', a.fill); el.style.fill = a.fill; }
  if (set(a.stroke)) { el.setAttribute('stroke', a.stroke); el.style.stroke = a.stroke; }
  if (set(a.opacity)) el.setAttribute('opacity', a.opacity);
  if (set(a.visible)) el.style.display = bool(a.visible) ? '' : 'none';
  if (a.text !== undefined && el.tagName.toLowerCase() === 'text') el.textContent = a.text ?? '';
  for (const [k, cls] of [['flow', 'sym-flow'], ['blink', 'sym-blink'], ['spin', 'sym-spin']]) {
    if (a[k] !== undefined) el.classList.toggle(cls, bool(a[k]));
  }
  if (set(a.rotate) || set(a.tx) || set(a.ty)) {
    if (!el.hasAttribute('data-t0')) el.setAttribute('data-t0', el.getAttribute('transform') || '');
    let t = '';
    if (set(a.tx) || set(a.ty)) t += `translate(${num(a.tx)} ${num(a.ty)}) `;
    if (set(a.rotate)) {
      let [px, py] = (el.getAttribute('data-pivot') || '').split(/[\s,]+/).map(Number);
      if (isNaN(px) || isNaN(py)) {
        try { const bb = el.getBBox(); px = bb.x + bb.width / 2; py = bb.y + bb.height / 2; } catch { px = 0; py = 0; }
      }
      t += `rotate(${num(a.rotate)} ${px} ${py}) `;
    }
    el.setAttribute('transform', t + el.getAttribute('data-t0'));
  }
}

// ------------------------------------------------------------------ simboli SVG

/** Sanifica un SVG (rimuove script, eventi, riferimenti esterni). */
export function sanitizeSvg(text) {
  const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
  const svg = doc.querySelector('svg');
  if (!svg || doc.querySelector('parsererror')) throw new Error('File SVG non valido');
  svg.querySelectorAll('script, foreignObject, iframe, embed, object').forEach((n) => n.remove());
  for (const el of [svg, ...svg.querySelectorAll('*')]) {
    for (const a of [...el.attributes]) {
      const n = a.name.toLowerCase(), v = a.value.trim().toLowerCase();
      if (n.startsWith('on')) el.removeAttribute(a.name);
      else if ((n === 'href' || n === 'xlink:href') && !v.startsWith('#') && !v.startsWith('data:image/')) el.removeAttribute(a.name);
      else if (v.includes('javascript:')) el.removeAttribute(a.name);
    }
    if (el.tagName.toLowerCase() === 'style') el.textContent = el.textContent.replace(/@import[^;]*;?/gi, '').replace(/url\((?!\s*['"]?#)[^)]*\)/gi, 'none');
  }
  if (!svg.getAttribute('viewBox')) {
    const w = parseFloat(svg.getAttribute('width')) || 100, h = parseFloat(svg.getAttribute('height')) || 100;
    svg.setAttribute('viewBox', `0 0 ${w} ${h}`);
  }
  return new XMLSerializer().serializeToString(svg);
}

const NON_BINDABLE = ['defs', 'lineargradient', 'radialgradient', 'stop', 'clippath', 'mask', 'style', 'title', 'desc'];

/** Prepara un simbolo: id unici per istanza (prefisso __P__), elementi marcati con data-el, metadati. */
export function compileSymbol(svgText) {
  const doc = new DOMParser().parseFromString(svgText, 'image/svg+xml');
  const svg = doc.querySelector('svg');
  const ids = [];
  svg.querySelectorAll('[id]').forEach((el) => {
    const id = el.getAttribute('id');
    ids.push({ id, tag: el.tagName.toLowerCase(), desc: el.getAttribute('data-desc') || '' });
    el.setAttribute('data-el', id);
    el.setAttribute('id', `__P__${id}`);
  });
  const title = svg.querySelector(':scope > title')?.textContent || '';
  svg.querySelector(':scope > title')?.remove();
  let body = svg.innerHTML;
  for (const { id } of ids) {
    const e = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    body = body.replace(new RegExp(`url\\(\\s*#${e}\\s*\\)`, 'g'), `url(#__P__${id})`)
      .replace(new RegExp(`href="#${e}"`, 'g'), `href="#__P__${id}"`);
  }
  return {
    viewBox: svg.getAttribute('viewBox') || '0 0 100 100', body, title,
    category: svg.getAttribute('data-category') || 'Importati',
    udt: svg.getAttribute('data-udt') || '', faceplate: svg.getAttribute('data-faceplate') || '',
    elements: ids.filter((x) => !NON_BINDABLE.includes(x.tag)),
  };
}

// ------------------------------------------------------------------ parametri e binding

/** Sostituisce {nome} con i valori dello scope (i nomi sconosciuti restano invariati). */
export function subst(s, scope) {
  if (typeof s !== 'string' || !scope || !s.includes('{')) return s;
  return s.replace(/\{([A-Za-z_][\w]*)\}/g, (m, k) => (Object.hasOwn(scope, k) && scope[k] !== undefined && scope[k] !== null ? String(scope[k]) : m));
}

const exprCache = new Map();
function compileExpr(src) {
  if (!exprCache.has(src)) {
    let f;
    try {
      // eslint-disable-next-line no-new-func
      f = new Function('value', 'tag', 'Math', 'param', `"use strict"; return (${src});`);
    } catch (e) {
      f = () => { throw e; };
    }
    if (exprCache.size > 5000) exprCache.clear();
    exprCache.set(src, f);
  }
  return exprCache.get(src);
}

function matchRule(rule, v) {
  const r = String(rule.in ?? '').trim();
  const n = +v;
  let m;
  if ((m = r.match(/^(-?[\d.]+)\s*\.\.\s*(-?[\d.]+)$/))) return !isNaN(n) && n >= +m[1] && n <= +m[2];
  if ((m = r.match(/^(>=|<=|>|<|!=)\s*(-?[\d.]+)$/))) {
    if (v === null || v === undefined || v === '' || isNaN(n)) return false;
    return { '>=': n >= +m[2], '<=': n <= +m[2], '>': n > +m[2], '<': n < +m[2], '!=': n != +m[2] }[m[1]];
  }
  if (r === 'true' || r === 'false') return bool(v) === (r === 'true') && (typeof v === 'boolean' || v === 0 || v === 1 || v === '0' || v === '1' || v === 'true' || v === 'false');
  return String(v) === r;
}

export function applyTransform(t, v, tagFn, scope = {}) {
  if (!t || !t.kind || t.kind === 'none') return v;
  if (t.kind === 'map') {
    for (const rule of t.rules || []) if (matchRule(rule, v)) return rule.out;
    return t.default !== undefined && t.default !== '' ? t.default : v;
  }
  if (t.kind === 'expr') return compileExpr(subst(t.expr || 'value', scope))(v, tagFn, Math, (k) => scope[k]);
  if (t.kind === 'format') {
    const val = typeof v === 'number' && t.decimals !== undefined && t.decimals !== '' ? v.toFixed(+t.decimals) : v;
    return String(t.format || '{value}').replaceAll('{value}', val ?? '');
  }
  return v;
}

/** Valuta un binding nello scope dato (parametri vista + proprietà custom). Ritorna {value, bad, error, tag}. */
export function evalBinding(b, live, scope = {}) {
  const tagFn = (n) => live.value(n);
  try {
    let v, bad = false, tag;
    if (b.kind === 'tag') {
      tag = subst(b.tag || '', scope);
      if (!tag || tag.includes('{')) return { value: undefined, tag, error: tag ? `parametro non risolto in '${tag}'` : undefined };
      const st = live.tags.get(tag);
      v = st?.value;
      bad = !st || st.quality !== 'good';
    } else if (b.kind === 'expr') {
      v = compileExpr(subst(b.expr || 'null', scope))(undefined, tagFn, Math, (k) => scope[k]);
    } else return { value: undefined };
    return { value: applyTransform(b.transform, v, tagFn, scope), bad, tag };
  } catch (e) {
    return { value: undefined, error: e.message, bad: true };
  }
}

function setPath(obj, path, v) {
  const parts = path.split('.');
  let o = obj;
  for (let i = 0; i < parts.length - 1; i++) {
    if (typeof o[parts[i]] !== 'object' || o[parts[i]] === null) o[parts[i]] = {};
    o = o[parts[i]];
  }
  o[parts.at(-1)] = v;
}

export function defaultsFor(type) {
  const d = COMPONENTS[type];
  return { visible: true, opacity: 1, tooltip: '', ...structuredClone(d?.props || {}) };
}

/** Scope di un componente: parametri della vista + proprietà custom (risolte nei parametri). */
export function scopeFor(comp, viewScope = {}) {
  const sc = { ...viewScope };
  for (const [k, v] of Object.entries(comp.custom || {})) sc[k] = subst(String(v ?? ''), viewScope);
  return sc;
}

/** Proprietà effettive di un componente (statiche + binding). */
export function resolveProps(comp, live, scope = {}) {
  const p = { ...defaultsFor(comp.type), ...structuredClone(comp.props || {}) };
  let bad = false;
  const errors = [];
  for (const [path, b] of Object.entries(comp.bindings || {})) {
    if (!b || !b.kind) continue;
    const r = evalBinding(b, live, scope);
    if (r.bad) bad = true;
    if (r.error) errors.push(`${path}: ${r.error}`);
    if (r.value !== undefined) setPath(p, path, r.value);
  }
  return { props: p, bad, errors };
}

/** Parametri effettivi di una vista: default dichiarati + valori passati. */
export function viewScope(view, passed = {}) {
  const sc = {};
  for (const [k, v] of Object.entries(view?.params || {})) sc[k] = v;
  for (const [k, v] of Object.entries(passed || {})) sc[k] = v;
  return sc;
}

// ------------------------------------------------------------------ renderer

/** Cache condivisa delle viste caricate (per viste incorporate e popup). */
const viewCache = new Map();
export function loadViewCached(api, id) {
  if (!viewCache.has(id)) viewCache.set(id, api.get(`/api/views/${encodeURIComponent(id)}`).catch((e) => { viewCache.delete(id); throw e; }));
  return viewCache.get(id);
}
export function invalidateView(id) { if (id) viewCache.delete(id); else viewCache.clear(); }

export class ViewRenderer {
  /**
   * @param {SVGSVGElement} svg  elemento radice
   * @param {Live} live
   * @param {{design?: boolean, preview?: Function, onAction?: Function, params?: object, api?: object, symbols?: object}} opts
   */
  constructor(svg, live, opts = {}) {
    this.svg = svg;
    this.live = live;
    this.opts = opts;
    this.symbols = opts.symbols || {};
    this.view = null;
    this.params = opts.params || {};
    this.scope = {};
    this.cache = new Map();
    this.children = new Map();
    this.layer = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    this.bg = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    svg.append(this.bg, this.layer);
  }

  setSymbols(list) {
    const out = {};
    for (const s of list) {
      try { out[s.id] = compileSymbol(s.svg); } catch (e) { console.warn('simbolo', s.id, e); }
    }
    this.symbols = out;
    this.cache.clear();
    return out;
  }

  setParams(params) {
    this.params = params || {};
    this.scope = viewScope(this.view, this.params);
    this.update(true);
  }

  load(view) {
    this.view = view;
    this.scope = viewScope(view, this.params);
    this.cache.clear();
    for (const ch of this.children.values()) ch.renderer?.destroy?.();
    this.children.clear();
    this.layer.innerHTML = '';
    this.svg.setAttribute('viewBox', `0 0 ${view.width} ${view.height}`);
    this.bg.setAttribute('width', view.width);
    this.bg.setAttribute('height', view.height);
    this.bg.setAttribute('fill', view.background || '#0b1220');
    this.update(true);
  }

  history(tag) { return this.live.history?.(tag) || []; }

  /** Ridisegna i componenti la cui uscita è cambiata. */
  update(force = false) {
    if (!this.view) return;
    const seen = new Set();
    const comps = this.view.components;
    comps.forEach((c, idx) => {
      seen.add(c.id);
      let g = this.layer.querySelector(`:scope > g[data-id="${CSS.escape(c.id)}"]`);
      if (!g) {
        g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        g.dataset.id = c.id;
        g.addEventListener('click', (e) => {
          if (!this.opts.design || this.opts.preview?.()) {
            const comp = this.view.components.find((x) => x.id === c.id);
            if (comp?.events?.onClick?.actions?.length) {
              e.stopPropagation();
              this.opts.onAction?.(comp, 'onClick', scopeFor(comp, this.scope));
            }
          }
        });
      }
      if (this.layer.children[idx] !== g) this.layer.insertBefore(g, this.layer.children[idx] || null);
      const def = COMPONENTS[c.type];
      if (!def) return;
      const scope = scopeFor(c, this.scope);
      const { props, bad, errors } = resolveProps(c, this.live, scope);
      if (c.type === 'trend') props.tags = subst(String(props.tags || ''), scope);
      const w = Math.max(1, num(c.w, def.size[0])), h = Math.max(1, num(c.h, def.size[1]));
      const inner = def.render(props, w, h, { symbols: this.symbols, uid: c.id, history: (t) => this.history(t) }) +
        (bad ? `<rect x="-3" y="-3" width="${w + 6}" height="${h + 6}" fill="none" stroke="#f59e0b" stroke-dasharray="3 3" class="badq"/>` : '');
      const transform = `translate(${num(c.x)} ${num(c.y)})${c.rotation ? ` rotate(${num(c.rotation)} ${w / 2} ${h / 2})` : ''}`;
      const key = inner + transform + props.visible + props.opacity + JSON.stringify(props.elements || '') + props.tooltip;
      if (c.type === 'trend') this.live.watchHistory?.(props.tags.split(',').map((t) => t.trim()).filter(Boolean));
      if (!force && this.cache.get(c.id) === key) {
        if (c.type === 'embed') this.updateEmbed(c, g, props, scope);
        return;
      }
      this.cache.set(c.id, key);
      g.setAttribute('transform', transform);
      g.style.display = bool(props.visible) || props.visible === undefined ? '' : 'none';
      if (this.opts.design && !bool(props.visible)) { g.style.display = ''; g.setAttribute('opacity', 0.3); } else g.setAttribute('opacity', num(props.opacity, 1));
      g.classList.toggle('clickable', !!c.events?.onClick?.actions?.length);
      g.innerHTML = (props.tooltip ? `<title>${esc(props.tooltip)}</title>` : '') + inner;
      if (errors.length) g.dataset.errors = errors.join('\n'); else delete g.dataset.errors;
      def.after?.(g, props);
      if (c.type === 'embed') this.updateEmbed(c, g, props, scope, true);
    });
    for (const g of [...this.layer.children]) {
      if (!seen.has(g.dataset.id)) { g.remove(); this.cache.delete(g.dataset.id); this.children.delete(g.dataset.id); }
    }
  }

  /** Vista incorporata: crea/aggiorna il renderer figlio passando i parametri. */
  updateEmbed(c, g, props, scope, rebuilt = false) {
    let params = {};
    try {
      const raw = typeof props.params === 'string' ? JSON.parse(props.params || '{}') : props.params || {};
      for (const [k, v] of Object.entries(raw)) params[k] = typeof v === 'string' ? subst(v, scope) : v;
    } catch { params = {}; }
    const host = g.querySelector('svg.embed-host');
    let ch = this.children.get(c.id);
    if (rebuilt || !ch || ch.host !== host || ch.viewId !== props.view) {
      if (!props.view || !host || !this.opts.api) { this.children.delete(c.id); return; }
      const r = new ViewRenderer(host, this.live, { ...this.opts, params, symbols: this.symbols });
      ch = { host, renderer: r, viewId: props.view, paramsKey: JSON.stringify(params) };
      this.children.set(c.id, ch);
      if ((this.depth || 0) > 4) return; // protezione da annidamenti ricorsivi
      r.depth = (this.depth || 0) + 1;
      loadViewCached(this.opts.api, props.view).then((v) => r.load(v)).catch(() => {
        host.innerHTML = `<text x="10" y="20" fill="#ef4444" font-size="12">vista '${esc(props.view)}' non trovata</text>`;
      });
      return;
    }
    const key = JSON.stringify(params);
    if (key !== ch.paramsKey) { ch.paramsKey = key; ch.renderer.setParams(params); } else ch.renderer.update();
  }
}

/** Esegue le azioni di un evento (runtime). `scope` = parametri del componente. */
export async function runActions(comp, evName, { live, api, navigate, toast, viewId, openPopup, scope = {} }) {
  const ev = comp.events?.[evName];
  if (!ev) return;
  if (ev.confirm && !confirm(subst(ev.confirm, scope))) return;
  for (const a of ev.actions || []) {
    try {
      if (a.kind === 'write') {
        const tag = subst(a.tag, scope);
        let value = a.value;
        if (value === 'toggle') value = !bool(live.value(tag));
        else if (value === 'prompt') {
          const s = prompt(`Nuovo valore per ${tag}`, live.value(tag) ?? '');
          if (s === null) return;
          value = s;
        } else if (typeof value === 'string') value = subst(value, scope);
        await api.post('/api/tags/write', { tag, value });
      } else if (a.kind === 'script') {
        let payload = {};
        try { payload = a.payload ? JSON.parse(subst(a.payload, scope)) : {}; } catch { toast('Payload JSON non valido', 'error'); return; }
        const r = await api.post('/api/scripts/call', { fn: a.fn, payload, source: `${viewId}/${comp.name || comp.id}` });
        const res = r.result;
        if (res && typeof res === 'object') {
          if (res.message) toast(res.message);
          if (res.navigate) navigate(res.navigate);
          if (res.popup) openPopup?.(res.popup, res.params || {}, res.title);
        }
        if (r.write_errors?.length) toast(r.write_errors.join('; '), 'error');
      } else if (a.kind === 'navigate') {
        navigate(a.view);
      } else if (a.kind === 'popup') {
        const params = {};
        for (const [k, v] of Object.entries(a.params || {})) params[k] = subst(String(v), scope);
        openPopup?.(a.view, params, subst(a.title || '', scope));
      }
    } catch (e) {
      toast(e.message, 'error');
      return;
    }
  }
}

/** Finestra popup flottante (faceplate) con una vista e i suoi parametri. */
export function createPopup({ live, api, toast, symbols, viewId, params, title, onAction, onClose }) {
  const win = document.createElement('div');
  win.className = 'popup-win';
  const n = document.querySelectorAll('.popup-win').length;
  win.style.left = `${120 + n * 30}px`;
  win.style.top = `${80 + n * 30}px`;
  win.innerHTML = `<div class="popup-head"><span class="popup-title"></span><span class="grow"></span><button class="small popup-close">✕</button></div>
    <div class="popup-body"><svg xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet"></svg></div>`;
  document.body.appendChild(win);
  win.querySelector('.popup-title').textContent = title || viewId;
  const svg = win.querySelector('svg');
  const r = new ViewRenderer(svg, live, { api, params, symbols, onAction });
  r.viewId = viewId;
  const close = () => { win.remove(); onClose?.(r); };
  win.querySelector('.popup-close').onclick = close;
  // trascinamento dalla barra del titolo
  const head = win.querySelector('.popup-head');
  head.addEventListener('mousedown', (e) => {
    if (e.target.closest('button')) return;
    const ox = e.clientX - win.offsetLeft, oy = e.clientY - win.offsetTop;
    const mv = (ev) => { win.style.left = `${ev.clientX - ox}px`; win.style.top = `${Math.max(0, ev.clientY - oy)}px`; };
    const up = () => { window.removeEventListener('mousemove', mv); window.removeEventListener('mouseup', up); };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  });
  win.addEventListener('mousedown', () => { document.querySelectorAll('.popup-win').forEach((w) => (w.style.zIndex = 600)); win.style.zIndex = 601; });
  loadViewCached(api, viewId).then((v) => {
    const k = Math.min(1, (window.innerHeight - 120) / v.height);
    svg.style.width = `${Math.round(v.width * k)}px`;
    svg.style.height = `${Math.round(v.height * k)}px`;
    if (!title) win.querySelector('.popup-title').textContent = v.title + (params.device ? ` – ${params.device}` : '');
    r.load(v);
  }).catch((e) => { toast(`Vista ${viewId}: ${e.message}`, 'error'); close(); });
  return { win, renderer: r, close };
}
