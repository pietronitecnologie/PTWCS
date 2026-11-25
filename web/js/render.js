// Renderer delle viste (condiviso tra Runtime e Designer).
//
// Modello vista:
// { id, title, width, height, background, home, components: [Component] }
// Component:
// { id, type, name, x, y, w, h, rotation,
//   props:    { ...proprietà statiche del tipo, visible, opacity, tooltip },
//   bindings: { "<path prop>": Binding },          // es. "running", "elements.body.fill"
//   events:   { onClick: { confirm?, actions: [Action] } } }
// Binding:
// { kind: "tag", tag: "CV1.Run", transform? }  |  { kind: "expr", expr: "tag('A') > 3", transform? }
// Transform:
// { kind: "map", rules: [{ in: "true" | "10..20" | ">5", out: "..." }], default }
// { kind: "expr", expr: "value / 10" }   |   { kind: "format", format: "{value} kg", decimals: 1 }
// Action:
// { kind: "write", tag, value }  (value: valore | "toggle" | "prompt")
// { kind: "script", fn, payload }  |  { kind: "navigate", view }

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
    // Applica le proprietà per elemento (props.elements.<id>.fill|stroke|visible|opacity|text|rotate)
    after(g, p) {
      for (const [id, a] of Object.entries(p.elements || {})) {
        g.querySelectorAll(`[data-el="${CSS.escape(id)}"]`).forEach((el) => {
          if (a.fill !== undefined && a.fill !== '') { el.setAttribute('fill', a.fill); el.style.fill = a.fill; }
          if (a.stroke !== undefined && a.stroke !== '') { el.setAttribute('stroke', a.stroke); el.style.stroke = a.stroke; }
          if (a.opacity !== undefined && a.opacity !== '') el.setAttribute('opacity', a.opacity);
          if (a.visible !== undefined && a.visible !== '') el.style.display = bool(a.visible) ? '' : 'none';
          if (a.text !== undefined && el.tagName.toLowerCase() === 'text') el.textContent = a.text;
          if (a.rotate !== undefined && a.rotate !== '') {
            const bb = el.getBBox();
            el.setAttribute('transform', `rotate(${num(a.rotate)} ${bb.x + bb.width / 2} ${bb.y + bb.height / 2})`);
          }
        });
      }
    },
  },
};

export const COMMON_SCHEMA = [['visible', 'bool', 'Visibile'], ['opacity', 'number', 'Opacità (0-1)'], ['tooltip', 'text', 'Tooltip']];
export const ELEMENT_ATTRS = ['fill', 'stroke', 'visible', 'opacity', 'text', 'rotate'];

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

/** Prepara un simbolo: id resi unici per istanza (prefisso __P__), elementi marcati con data-el. */
export function compileSymbol(svgText) {
  const doc = new DOMParser().parseFromString(svgText, 'image/svg+xml');
  const svg = doc.querySelector('svg');
  const ids = [];
  svg.querySelectorAll('[id]').forEach((el) => {
    const id = el.getAttribute('id');
    ids.push({ id, tag: el.tagName.toLowerCase() });
    el.setAttribute('data-el', id);
    el.setAttribute('id', `__P__${id}`);
  });
  let body = svg.innerHTML;
  for (const { id } of ids) {
    const e = id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    body = body.replace(new RegExp(`url\\(\\s*#${e}\\s*\\)`, 'g'), `url(#__P__${id})`)
      .replace(new RegExp(`href="#${e}"`, 'g'), `href="#__P__${id}"`);
  }
  // Classi CSS interne potrebbero collidere tra simboli: prefissiamo i selettori #id.
  return { viewBox: svg.getAttribute('viewBox') || '0 0 100 100', body, elements: ids.filter((x) => !['defs', 'lineargradient', 'radialgradient', 'stop', 'clippath', 'mask', 'style'].includes(x.tag)) };
}

// ------------------------------------------------------------------ binding

const exprCache = new Map();
function compileExpr(src) {
  if (!exprCache.has(src)) {
    let f;
    try {
      // eslint-disable-next-line no-new-func
      f = new Function('value', 'tag', 'Math', `"use strict"; return (${src});`);
    } catch (e) {
      f = () => { throw e; };
    }
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
    if (isNaN(n)) return false;
    return { '>=': n >= +m[2], '<=': n <= +m[2], '>': n > +m[2], '<': n < +m[2], '!=': n != +m[2] }[m[1]];
  }
  if (r === 'true' || r === 'false') return bool(v) === (r === 'true') && (typeof v === 'boolean' || v === 0 || v === 1 || v === '0' || v === '1' || v === 'true' || v === 'false');
  return String(v) === r;
}

export function applyTransform(t, v, tagFn) {
  if (!t || !t.kind || t.kind === 'none') return v;
  if (t.kind === 'map') {
    for (const rule of t.rules || []) if (matchRule(rule, v)) return rule.out;
    return t.default !== undefined && t.default !== '' ? t.default : v;
  }
  if (t.kind === 'expr') return compileExpr(t.expr || 'value')(v, tagFn, Math);
  if (t.kind === 'format') {
    const val = typeof v === 'number' && t.decimals !== undefined && t.decimals !== '' ? v.toFixed(+t.decimals) : v;
    return String(t.format || '{value}').replaceAll('{value}', val ?? '');
  }
  return v;
}

/** Valuta un binding. Ritorna {value, bad, error}. */
export function evalBinding(b, live) {
  const tagFn = (n) => live.value(n);
  try {
    let v, bad = false;
    if (b.kind === 'tag') {
      const st = live.tags.get(b.tag);
      v = st?.value;
      bad = !st || st.quality !== 'good';
    } else if (b.kind === 'expr') {
      v = compileExpr(b.expr || 'null')(undefined, tagFn, Math);
    } else return { value: undefined };
    return { value: applyTransform(b.transform, v, tagFn), bad };
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
  return { visible: true, opacity: 1, tooltip: '', ...structuredClone(d.props) };
}

/** Proprietà effettive di un componente (statiche + binding). */
export function resolveProps(comp, live) {
  const p = { ...defaultsFor(comp.type), ...structuredClone(comp.props || {}) };
  let bad = false;
  const errors = [];
  for (const [path, b] of Object.entries(comp.bindings || {})) {
    if (!b || !b.kind) continue;
    const r = evalBinding(b, live);
    if (r.bad) bad = true;
    if (r.error) errors.push(`${path}: ${r.error}`);
    if (r.value !== undefined) setPath(p, path, r.value);
  }
  return { props: p, bad, errors };
}

// ------------------------------------------------------------------ renderer

export class ViewRenderer {
  /**
   * @param {SVGSVGElement} svg  elemento radice
   * @param {Live} live
   * @param {{design?: boolean, onAction?: Function}} opts
   */
  constructor(svg, live, opts = {}) {
    this.svg = svg;
    this.live = live;
    this.opts = opts;
    this.symbols = {};
    this.view = null;
    this.cache = new Map();
    this.layer = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    this.bg = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    svg.append(this.bg, this.layer);
  }

  setSymbols(list) {
    this.symbols = {};
    for (const s of list) {
      try { this.symbols[s.id] = compileSymbol(s.svg); } catch (e) { console.warn('simbolo', s.id, e); }
    }
    this.cache.clear();
  }

  load(view) {
    this.view = view;
    this.cache.clear();
    this.layer.innerHTML = '';
    this.svg.setAttribute('viewBox', `0 0 ${view.width} ${view.height}`);
    Object.assign(this.bg, {});
    this.bg.setAttribute('width', view.width);
    this.bg.setAttribute('height', view.height);
    this.bg.setAttribute('fill', view.background || '#0b1220');
    this.update(true);
  }

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
            if (comp?.events?.onClick?.actions?.length) { e.stopPropagation(); this.opts.onAction?.(comp, 'onClick'); }
          }
        });
      }
      // ordine z = ordine nell'array
      if (this.layer.children[idx] !== g) this.layer.insertBefore(g, this.layer.children[idx] || null);
      const def = COMPONENTS[c.type];
      if (!def) return;
      const { props, bad, errors } = resolveProps(c, this.live);
      const w = Math.max(1, num(c.w, def.size[0])), h = Math.max(1, num(c.h, def.size[1]));
      const inner = def.render(props, w, h, { symbols: this.symbols, uid: c.id }) +
        (bad ? `<rect x="-3" y="-3" width="${w + 6}" height="${h + 6}" fill="none" stroke="#f59e0b" stroke-dasharray="3 3" class="badq"/>` : '');
      const transform = `translate(${num(c.x)} ${num(c.y)})${c.rotation ? ` rotate(${num(c.rotation)} ${w / 2} ${h / 2})` : ''}`;
      const key = inner + transform + props.visible + props.opacity + JSON.stringify(props.elements || '') + props.tooltip;
      if (!force && this.cache.get(c.id) === key) return;
      this.cache.set(c.id, key);
      g.setAttribute('transform', transform);
      g.style.display = bool(props.visible) || props.visible === undefined ? '' : 'none';
      if (this.opts.design && !bool(props.visible)) { g.style.display = ''; g.setAttribute('opacity', 0.3); } else g.setAttribute('opacity', num(props.opacity, 1));
      g.classList.toggle('clickable', !!c.events?.onClick?.actions?.length);
      g.innerHTML = (props.tooltip ? `<title>${esc(props.tooltip)}</title>` : '') + inner;
      if (errors.length) g.dataset.errors = errors.join('\n'); else delete g.dataset.errors;
      def.after?.(g, props);
    });
    for (const g of [...this.layer.children]) if (!seen.has(g.dataset.id)) { g.remove(); this.cache.delete(g.dataset.id); }
  }
}

/** Esegue le azioni di un evento (runtime). */
export async function runActions(comp, evName, { live, api, navigate, toast, viewId }) {
  const ev = comp.events?.[evName];
  if (!ev) return;
  if (ev.confirm && !confirm(ev.confirm)) return;
  for (const a of ev.actions || []) {
    try {
      if (a.kind === 'write') {
        let value = a.value;
        if (value === 'toggle') value = !bool(live.value(a.tag));
        else if (value === 'prompt') {
          const s = prompt(`Nuovo valore per ${a.tag}`, live.value(a.tag) ?? '');
          if (s === null) return;
          value = s;
        }
        await api.post('/api/tags/write', { tag: a.tag, value });
      } else if (a.kind === 'script') {
        let payload = {};
        try { payload = a.payload ? JSON.parse(a.payload) : {}; } catch { toast('Payload JSON non valido', 'error'); return; }
        const r = await api.post('/api/scripts/call', { fn: a.fn, payload, source: `${viewId}/${comp.name || comp.id}` });
        const res = r.result;
        if (res && typeof res === 'object') {
          if (res.message) toast(res.message);
          if (res.navigate) navigate(res.navigate);
        }
        if (r.write_errors?.length) toast(r.write_errors.join('; '), 'error');
      } else if (a.kind === 'navigate') {
        navigate(a.view);
      }
    } catch (e) {
      toast(e.message, 'error');
      return;
    }
  }
}
