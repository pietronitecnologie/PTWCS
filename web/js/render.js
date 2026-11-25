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
      // stessa scala della libreria: rulli larghi h/6 con passo h/4 (più lungo = più rulli)
      const r = Math.min(h / 2, 14);
      let rollers = '';
      const pitch = h / 4, rw = h / 6;
      for (let x = pitch / 2; x < w - rw; x += pitch) rollers += `<rect x="${(x - rw / 2).toFixed(1)}" y="${(h * 0.12).toFixed(1)}" width="${rw.toFixed(1)}" height="${(h * 0.76).toFixed(1)}" rx="${(rw / 3).toFixed(1)}" fill="#0f172a" opacity=".35"/>`;
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
      // length: lunghezza variabile in scala (pattern ripetuti); uniform: proporzioni fisse; altrimenti stira (SVG importati)
      let vb = s.viewBox, par = 'none';
      if (s.fit === 'length') { const vh = s.vb[3]; vb = `${s.vb[0]} ${s.vb[1]} ${(w * vh / h).toFixed(2)} ${vh}`; }
      else if (s.fit === 'uniform') par = 'xMidYMid meet';
      return `<svg width="${w}" height="${h}" viewBox="${esc(vb)}" preserveAspectRatio="${par}" overflow="visible"
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
  whmap: {
    label: 'Mappa magazzino', icon: '▦', size: [500, 60],
    props: { wh: '', aisle: 1, side: 1, mode: 'plan', labels: true, highlight: '', title: '' },
    schema: [['wh', 'text', 'Magazzino'], ['aisle', 'text', 'Corsia ({param} ammesso)'], ['side', 'number', 'Lato'],
      ['mode', 'select', 'Modalità', ['plan', 'blocks', 'cells']], ['labels', 'bool', 'Etichette'], ['highlight', 'text', 'Evidenzia UdC'], ['title', 'text', 'Titolo']],
    render(p, w, h, ctx) {
      const aisle = Math.round(num(p.aisle, 1)), side = Math.round(num(p.side, 1));
      const box = `<rect width="${w}" height="${h}" fill="#0a1120" stroke="#24324f"/>`;
      if (!p.wh || !ctx.wh) return box + textEl(w / 2, h / 2, 'magazzino?', { size: 12, color: '#64748b' });
      if (p.mode === 'cells') {
        const d = ctx.wh(`${p.wh}|side|${aisle}|${side}`, `/api/wh/${encodeURIComponent(p.wh)}/side?aisle=${aisle}&side=${side}`);
        if (!d) return box + textEl(w / 2, h / 2, 'caricamento…', { size: 12, color: '#64748b' });
        const L = p.labels ? 26 : 0, B = p.labels ? 16 : 0, T = p.title ? 18 : 0;
        const cw = (w - L) / d.columns, ch = (h - B - T) / d.levels;
        let out = box + (p.title ? textEl(L, 9, p.title, { size: 12, anchor: 'start', color: '#cbd5e1' }) : '');
        for (let l = 0; l < d.levels; l++) {
          const y = T + (d.levels - 1 - l) * ch;
          if (p.labels) out += textEl(L - 4, y + ch / 2, `${l + 1}`, { size: Math.min(10, ch * 0.7), anchor: 'end', color: '#64748b' });
          for (let c = 0; c < d.columns; c++) {
            const st = d.states[l][c], lu = d.lus[l][c];
            const hl = p.highlight && lu === p.highlight;
            out += `<rect class="whcell" data-c="${c + 1}" data-l="${l + 1}" x="${(L + c * cw + 0.6).toFixed(1)}" y="${(y + 0.6).toFixed(1)}" width="${(cw - 1.2).toFixed(1)}" height="${(ch - 1.2).toFixed(1)}" rx="1.5"
              fill="${WH_COLORS[st]}" ${hl ? 'stroke="#fff" stroke-width="2.5"' : ''}><title>${esc(`C${aisle} L${side} col ${c + 1} liv ${l + 1}: ${WH_STATES[st]}${lu ? ' – ' + lu : ''}`)}</title></rect>`;
            if (st === 4) out += `<path d="M${L + c * cw + 2} ${y + 2} l${cw - 4} ${ch - 4} m0 ${-(ch - 4)} l${-(cw - 4)} ${ch - 4}" stroke="#fca5a5" stroke-width="1" pointer-events="none"/>`;
          }
        }
        if (p.labels) for (let c = 0; c < d.columns; c += 5) out += textEl(L + c * cw + cw / 2, h - 7, `${c + 1}`, { size: 9, color: '#64748b' });
        return out;
      }
      const sum = ctx.wh(`${p.wh}|summary`, `/api/wh/${encodeURIComponent(p.wh)}/summary`);
      const sd = sum?.aisles?.[aisle - 1]?.sides?.[side - 1];
      if (!sd) return box + textEl(w / 2, h / 2, sum ? `corsia ${aisle} / lato ${side}?` : 'caricamento…', { size: 12, color: '#64748b' });
      const nb = sd.blocks[0].length, nl = sd.blocks.length;
      const T = p.title ? 16 : 0;
      let out = box + (p.title ? textEl(4, 8, p.title, { size: 11, anchor: 'start', color: '#94a3b8' }) : '');
      const bw = w / nb;
      if (p.mode === 'blocks') {
        const bh = (h - T) / nl;
        for (let l = 0; l < nl; l++) for (let b = 0; b < nb; b++) {
          const f = sd.blocks[l][b];
          out += `<rect x="${(b * bw + 0.5).toFixed(1)}" y="${(T + (nl - 1 - l) * bh + 0.5).toFixed(1)}" width="${(bw - 1).toFixed(1)}" height="${(bh - 1).toFixed(1)}" fill="${fillColor(f)}"><title>blocco ${b + 1} liv ${l + 1}: ${Math.round(f * 100)}%</title></rect>`;
        }
        return out;
      }
      // pianta: saturazione media per blocco di colonne (tutti i livelli)
      for (let b = 0; b < nb; b++) {
        const f = sd.blocks.reduce((a, r) => a + r[b], 0) / nl;
        out += `<rect x="${(b * bw + 0.5).toFixed(1)}" y="${T + 0.5}" width="${(bw - 1).toFixed(1)}" height="${(h - T - 1).toFixed(1)}" fill="${fillColor(f)}"><title>colonne ${b * sum.geometry.block_cols + 1}-${Math.min((b + 1) * sum.geometry.block_cols, sum.geometry.columns)}: ${Math.round(f * 100)}%</title></rect>`;
        if (p.labels && bw > 22) out += textEl(b * bw + bw / 2, T + (h - T) / 2, `${Math.round(f * 100)}`, { size: Math.min(11, (h - T) * 0.5), color: '#e2e8f0' });
      }
      const tot = sd.free + sd.occupied + sd.reserved + sd.blocked;
      if (p.labels) out += textEl(w - 4, -7, `lato ${side}: ${sd.occupied + sd.reserved}/${tot} (${Math.round((sd.occupied + sd.reserved) * 100 / tot)}%)`, { size: 10, anchor: 'end', color: '#94a3b8' });
      return out;
    },
    after(g, p, r, c) {
      if (p.mode !== 'cells') return;
      g.querySelectorAll('.whcell').forEach((el) => (el.onclick = (e) => {
        e.stopPropagation();
        r?.opts?.onWhCell?.(p.wh, { aisle: Math.round(num(p.aisle, 1)), side: Math.round(num(p.side, 1)), col: +el.dataset.c, lvl: +el.dataset.l });
      }));
    },
  },
  whsummary: {
    label: 'KPI magazzino', icon: '▤', size: [300, 200],
    props: { wh: '', aisle: '', fontSize: 13 },
    schema: [['wh', 'text', 'Magazzino'], ['aisle', 'text', 'Corsia (vuoto = totale)'], ['fontSize', 'number', 'Dim. font']],
    render(p, w, h, ctx) {
      const sum = p.wh && ctx.wh ? ctx.wh(`${p.wh}|summary`, `/api/wh/${encodeURIComponent(p.wh)}/summary`) : null;
      let out = `<rect width="${w}" height="${h}" rx="6" fill="#0f172a" stroke="#24324f"/>`;
      if (!sum) return out + textEl(w / 2, h / 2, p.wh ? 'caricamento…' : 'magazzino?', { size: 12, color: '#64748b' });
      const fs = num(p.fontSize, 13);
      let rows;
      if (p.aisle !== '' && p.aisle !== undefined && p.aisle !== null) {
        const a = sum.aisles[Math.round(num(p.aisle, 1)) - 1];
        if (!a) return out + textEl(w / 2, h / 2, 'corsia?', { size: 12 });
        const t = a.sides.reduce((x, s) => ({ free: x.free + s.free, occ: x.occ + s.occupied + s.reserved, bl: x.bl + s.blocked }), { free: 0, occ: 0, bl: 0 });
        const tot = t.free + t.occ + t.bl;
        if (h < 90) {
          // modalità compatta (due righe)
          return out + textEl(10, h * 0.3, `${t.occ}/${tot} occupate (${Math.round(t.occ * 100 / tot)}%) · ${t.free} libere${t.bl ? ` · ${t.bl} bloccate` : ''}`, { size: fs, anchor: 'start' })
            + textEl(10, h * 0.72, `coda ${a.queued} · ${a.done_hour} missioni/ora`, { size: fs, anchor: 'start', color: '#94a3b8' });
        }
        rows = [['Corsia', `${a.name} · traslo ${a.crane}`], ['Locazioni', tot], ['Occupate', `${t.occ} (${Math.round(t.occ * 100 / tot)}%)`], ['Libere', t.free], ['Bloccate', t.bl],
          ['Missioni in coda', a.queued],
          ['In corso' + (a.slots === 2 ? ' (posto 1)' : ''), a.active ? `#${a.active.id} ${a.active.kind === 'in' ? 'deposito' : 'prelievo'} ${a.active.lu}` : '—'],
          ...(a.slots === 2 ? [['In corso (posto 2)', a.active2 ? `#${a.active2.id} ${a.active2.kind === 'in' ? 'deposito' : 'prelievo'} ${a.active2.lu}` : '—']] : []),
          ['Completate/ora', a.done_hour]];
      } else {
        const t = sum.totals, m = sum.missions;
        rows = [['Locazioni', sum.geometry.total], ['Occupate', t.occupied], ['Prenotate', t.reserved], ['Libere', t.free], ['Bloccate', t.blocked],
          ['Saturazione', `${t.fill_pct.toFixed(1)} %`], ['Missioni coda / in corso', `${m.queued} / ${m.running}`], ['Depositi / prelievi ora', `${m.in_hour} / ${m.out_hour}`]];
      }
      const rh = (h - 10) / rows.length;
      rows.forEach(([k, v], i) => {
        out += textEl(10, 5 + rh * i + rh / 2, k, { size: fs, anchor: 'start', color: '#94a3b8' }) + textEl(w - 10, 5 + rh * i + rh / 2, String(v), { size: fs, anchor: 'end', bold: true });
      });
      return out;
    },
  },
  whmissions: {
    label: 'Missioni magazzino', icon: '☰', size: [520, 220],
    props: { wh: '', rows: 8, fontSize: 11 },
    schema: [['wh', 'text', 'Magazzino'], ['rows', 'number', 'Righe'], ['fontSize', 'number', 'Dim. font']],
    render(p, w, h, ctx) {
      const d = p.wh && ctx.wh ? ctx.wh(`${p.wh}|missions`, `/api/wh/${encodeURIComponent(p.wh)}/missions?limit=20`) : null;
      let out = `<rect width="${w}" height="${h}" rx="6" fill="#0f172a" stroke="#24324f"/>`;
      if (!d) return out + textEl(w / 2, h / 2, p.wh ? 'caricamento…' : 'magazzino?', { size: 12, color: '#64748b' });
      const fs = num(p.fontSize, 11), n = Math.max(1, num(p.rows, 8));
      const open = [...d.open].sort((a, b) => (a.state === 'queued') - (b.state === 'queued') || a.id - b.id);
      const done = d.recent.filter((m) => m.state === 'done');
      const list = [...open, ...done].slice(0, n);
      const cols = [8, 0.1, 0.23, 0.42, 0.7, 0.86].map((x) => (x < 1 ? x * w : x));
      const hdr = ['#', 'Tipo', 'UdC', 'Da → A', 'Stato', 'Corsia'];
      const rh = (h - 22) / n;
      hdr.forEach((t, i) => (out += textEl(cols[i], 12, t, { size: fs, anchor: 'start', color: '#64748b', bold: true })));
      const stc = { queued: '#94a3b8', sent: '#38bdf8', running: '#22c55e', done: '#64748b', cancelled: '#ef4444' };
      const stl = { queued: 'in coda', sent: 'inviata', running: 'in corso', done: 'completata', cancelled: 'annullata' };
      const code = (pp) => (pp.col === 0 ? 'P&D' : `${pp.side}-${String(pp.col).padStart(2, '0')}-${String(pp.lvl).padStart(2, '0')}`);
      list.forEach((m, i) => {
        const y = 22 + rh * i + rh / 2;
        const vals = [m.id, m.kind === 'in' ? '⬇ dep.' : '⬆ prel.', m.lu, m.kind === 'in' ? `${m.bay} → ${code(m.dst)}` : `${code(m.src)} → ${m.bay}`, stl[m.state],
          m.slot && m.state !== 'queued' ? `${m.aisle} · posto ${m.slot}` : m.aisle];
        vals.forEach((v, k) => (out += textEl(cols[k], y, String(v), { size: fs, anchor: 'start', color: k === 4 ? stc[m.state] : '#e2e8f0' })));
      });
      return out;
    },
  },
};
const WH_COLORS = ['#1e293b', '#b7793e', '#3b82f6', '#22c55e', '#7f1d1d'];
const WH_STATES = ['libera', 'occupata', 'prenotata (deposito)', 'in prelievo', 'bloccata'];
const FILL_STOPS = [[0, [30, 41, 59]], [0.35, [30, 64, 175]], [0.6, [59, 130, 246]], [0.8, [20, 184, 166]], [0.92, [245, 158, 11]], [1, [239, 68, 68]]];
function fillColor(f) {
  f = Math.max(0, Math.min(1, f));
  for (let i = 1; i < FILL_STOPS.length; i++) {
    const [a, ca] = FILL_STOPS[i - 1], [b, cb] = FILL_STOPS[i];
    if (f <= b) {
      const k = (f - a) / (b - a);
      return `rgb(${ca.map((x, j) => Math.round(x + (cb[j] - x) * k)).join(',')})`;
    }
  }
  return 'rgb(239,68,68)';
}
export const FILL_LEGEND = [0, 0.35, 0.6, 0.8, 0.92, 1].map((f) => [Math.round(f * 100), fillColor(f)]);
export const WH_LEGEND = WH_STATES.map((s, i) => [s, WH_COLORS[i]]);

// Cache dati magazzino (API), condivisa da tutti i renderer, aggiornata ogni 1,5 s.
const whCache = new Map();
const renderers = new Set();
let whTimer = null;
function whFetch(api, key, url, touch = true) {
  const e = whCache.get(key) || { data: null, ts: 0, used: Date.now(), busy: false, url };
  if (touch) e.used = Date.now();
  whCache.set(key, e);
  if (!e.busy && Date.now() - e.ts > 1400) {
    e.busy = true;
    api.get(url).then((d) => { e.data = d; e.ts = Date.now(); }).catch(() => { e.ts = Date.now(); }).finally(() => {
      e.busy = false;
      renderers.forEach((r) => r.update());
    });
  }
  if (!whTimer) whTimer = setInterval(() => {
    for (const [k, x] of whCache) {
      if (Date.now() - x.used > 6000) { whCache.delete(k); continue; }
      if (x.api && Date.now() - x.ts > 1400) whFetch(x.api, k, x.url, false);
    }
  }, 1500);
  e.api = api;
  return e.data;
}
export function whInvalidate() { for (const e of whCache.values()) e.ts = 0; }

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

const NON_BINDABLE = ['defs', 'lineargradient', 'radialgradient', 'stop', 'clippath', 'mask', 'style', 'title', 'desc', 'pattern'];

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
  const viewBox = svg.getAttribute('viewBox') || '0 0 100 100';
  const native = (svg.getAttribute('data-native') || '').split(/[\s,]+/).map(Number).filter((x) => x > 0);
  return {
    viewBox, body, title, vb: viewBox.split(/[\s,]+/).map(Number),
    fit: svg.getAttribute('data-fit') || '', native: native.length === 2 ? native : null,
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
    renderers.add(this);
  }

  destroy() { renderers.delete(this); }

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
      if (c.type === 'symbol') {
        // lunghezza corrente (unità del simbolo) e scala, per i binding dipendenti dalla lunghezza
        const sd = this.symbols[c.props?.symbol];
        if (sd) {
          const wpx = Math.max(1, num(c.w, 100)), hpx = Math.max(1, num(c.h, 100));
          scope._scale = sd.fit === 'length' ? hpx / sd.vb[3] : Math.min(wpx / sd.vb[2], hpx / sd.vb[3]);
          scope._len = sd.fit === 'length' ? wpx / scope._scale : sd.vb[2];
        }
      }
      const { props, bad, errors } = resolveProps(c, this.live, scope);
      if (c.type === 'trend') props.tags = subst(String(props.tags || ''), scope);
      for (const k of ['wh', 'aisle', 'highlight', 'title']) if (typeof props[k] === 'string') props[k] = subst(props[k], scope);
      const w = Math.max(1, num(c.w, def.size[0])), h = Math.max(1, num(c.h, def.size[1]));
      const inner = def.render(props, w, h, {
        symbols: this.symbols, uid: c.id, history: (t) => this.history(t),
        wh: this.opts.api ? (key, url) => whFetch(this.opts.api, key, url) : null,
      }) +
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
      def.after?.(g, props, this, c);
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
        const params = {};
        for (const [k, v] of Object.entries(a.params || {})) params[k] = subst(String(v), scope);
        navigate(a.view, params);
      } else if (a.kind === 'wh') {
        await whAction(a, scope, { api, toast, navigate });
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
export function createPopup({ live, api, toast, symbols, viewId, params, title, onAction, onClose, onWhCell }) {
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
  const r = new ViewRenderer(svg, live, { api, params, symbols, onAction, onWhCell });
  r.viewId = viewId;
  const close = () => { win.remove(); r.destroy(); onClose?.(r); };
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


// ------------------------------------------------------------------ azioni e dialoghi magazzino

/** Azioni operatore sul magazzino: ricerca UdC, prelievo, deposito manuale. */
export async function whAction(a, scope, { api, toast, navigate }) {
  const wh = subst(a.wh || '', scope);
  if (a.op === 'search') {
    const lu = prompt('Identificativo UdC da cercare:');
    if (!lu) return;
    const r = await api.get(`/api/wh/${encodeURIComponent(wh)}/find?lu=${encodeURIComponent(lu.trim())}`);
    if (!r.found) return toast(`UdC ${lu} non presente`, 'error');
    toast(`${lu}: ${r.code} (${r.loc.state})`, 'ok');
    if (a.view) navigate(a.view, { aisle: String(r.pos.aisle), hl: lu.trim() });
  } else if (a.op === 'outbound') {
    const lu = prompt('UdC da prelevare (vuoto = una a caso):', '');
    if (lu === null) return;
    const bay = a.bay ? subst(a.bay, scope) : prompt('Baia d\'uscita (vuoto = bilanciata):', '') ?? '';
    const m = await api.post(`/api/wh/${encodeURIComponent(wh)}/outbound`, { lu: lu.trim() || null, bay: bay.trim() || null });
    toast(`Missione ${m.id}: prelievo ${m.lu} → ${m.bay}`, 'ok');
    whInvalidate();
  } else if (a.op === 'inbound') {
    const lu = prompt('UdC da depositare (identificativo):');
    if (!lu) return;
    const r = await api.post(`/api/wh/${encodeURIComponent(wh)}/inbound`, { lu: lu.trim(), bay: 'manuale' });
    toast(`Deposito ${lu} → ${r.code}`, 'ok');
    whInvalidate();
  }
}

/** Dialogo dettaglio cella (click sul prospetto). */
export async function whCellDialog(wh, pos, { api, toast }) {
  const q = `aisle=${pos.aisle}&side=${pos.side}&col=${pos.col}&lvl=${pos.lvl}`;
  const r = await api.get(`/api/wh/${encodeURIComponent(wh)}/location?${q}`);
  const sum = await api.get(`/api/wh/${encodeURIComponent(wh)}/summary`);
  const back = document.createElement('div');
  back.className = 'modal-back';
  const st = r.loc.state;
  back.innerHTML = `<div class="modal"><div class="mh">Locazione ${esc(r.code)}</div><div class="mb">
      <div class="field"><label>Magazzino</label><div>${esc(wh)}</div></div>
      <div class="field"><label>Corsia / lato</label><div>${pos.aisle} / ${pos.side}</div></div>
      <div class="field"><label>Colonna / livello</label><div>${pos.col} / ${pos.lvl}</div></div>
      <div class="field"><label>Stato</label><div><b>${esc(st)}</b></div></div>
      <div class="field"><label>UdC</label><div class="mono">${esc(r.loc.lu || '—')}</div></div>
      <div class="field"><label>Ultima variazione</label><div>${new Date(r.loc.ts).toLocaleString()}</div></div>
      ${st === 'occupied' ? `<div class="field"><label>Baia d'uscita</label><select id="whBay"><option value="">(bilanciata)</option>${sum.outputs.map((o) => `<option>${esc(o)}</option>`).join('')}</select></div>` : ''}
    </div><div class="mf"></div></div>`;
  const close = () => back.remove();
  const btn = (label, cls, fn) => {
    const b = document.createElement('button');
    b.textContent = label;
    if (cls) b.className = cls;
    b.onclick = async () => { try { await fn(); whInvalidate(); close(); } catch (e) { toast(e.message, 'error'); } };
    back.querySelector('.mf').appendChild(b);
  };
  if (st === 'occupied') btn('⬆ Preleva', 'primary', async () => {
    const m = await api.post(`/api/wh/${encodeURIComponent(wh)}/outbound`, { lu: r.loc.lu, bay: back.querySelector('#whBay').value || null });
    toast(`Missione ${m.id}: prelievo ${m.lu} → ${m.bay}`, 'ok');
  });
  if (st === 'free' || st === 'occupied') btn('⛔ Blocca', 'danger', () => api.post(`/api/wh/${encodeURIComponent(wh)}/location`, { ...pos, action: 'block' }));
  if (st === 'blocked') btn('✓ Sblocca', '', () => api.post(`/api/wh/${encodeURIComponent(wh)}/location`, { ...pos, action: 'unblock' }));
  btn('Chiudi', '', async () => {});
  back.addEventListener('mousedown', (e) => { if (e.target === back) close(); });
  document.body.appendChild(back);
}
