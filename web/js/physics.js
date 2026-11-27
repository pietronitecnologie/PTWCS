// Layer fisico: schema delle proprietà per tipo, geometria dei componenti e disegno
// (annotazioni nel Designer, unità di carico in movimento nel runtime).
import { esc } from './core.js';

const NS = 'http://www.w3.org/2000/svg';

/** Tipi fisici: [chiave, tipo campo, etichetta, opzioni/aiuto]. Tipi: tag (attuatore), stag (sensore), num, text, bool, comp:<tipi>, outputs. */
export const KINDS = {
  conveyor: {
    label: 'Nastro / rulliera', icon: '▭', color: '#22d3ee',
    help: 'Percorso lungo l\'asse del componente (sinistra → destra, o al contrario con direzione "left"). Il PLC comanda la marcia; le unità si muovono, accelerano e si accumulano.',
    fields: [
      ['run', 'tag', 'Marcia (attuatore)'], ['next', 'comp:conveyor,diverter,sink', 'Successivo'], ['sink', 'bool', 'Uscita dall\'impianto a fine nastro'],
      ['length_m', 'num', 'Lunghezza (m)', 'vuoto = dal disegno e dalla scala della vista'], ['speed', 'num', 'Velocità max (m/s)', '0,5'],
      ['accel', 'num', 'Accelerazione (m/s²)', '1'], ['decel', 'num', 'Decelerazione (m/s²)', '1,5'], ['gap_m', 'num', 'Distanza in accumulo (m)', '0,02'],
      ['speed_sp', 'tag', 'Setpoint velocità (attuatore)'], ['sp_max', 'num', 'Valore setpoint = velocità max', '100'],
      ['speed_fb', 'stag', 'Velocità reale (sensore)'], ['fb_scale', 'num', 'Scala velocità reale (valore per 1 m/s)', '1'],
    ],
  },
  photocell: {
    label: 'Fotocellula', icon: '⦿', color: '#f59e0b',
    help: 'Vera quando un\'unità interrompe il raggio nella posizione indicata (anche un\'unità a cavallo tra due nastri).',
    fields: [['tag', 'stag', 'Segnale (sensore)'], ['invert', 'bool', 'Contatto NC (vero a fotocellula libera)'], ['on', 'comp:conveyor', 'Montata su'], ['at_m', 'num', 'Posizione (m dall\'inizio)', 'vuoto = dal disegno']],
  },
  scanner: {
    label: 'Scanner barcode', icon: '▥', color: '#a78bfa',
    help: 'Quando il fronte di un\'unità passa dalla posizione scrive barcode e peso, poi incrementa il contatore (il PLC rileva la nuova lettura dal contatore).',
    fields: [['code_tag', 'stag', 'Barcode (sensore)'], ['count_tag', 'stag', 'Contatore letture (sensore)'], ['weight_tag', 'stag', 'Peso (sensore)'],
      ['weight_scale', 'num', 'Scala peso (valore per 1 kg)', '1'], ['noread_pct', 'num', 'Mancate letture (%)', '0'], ['noread_text', 'text', 'Testo mancata lettura', 'NOREAD'],
      ['on', 'comp:conveyor', 'Montato su'], ['at_m', 'num', 'Posizione (m dall\'inizio)', 'vuoto = dal disegno']],
  },
  diverter: {
    label: 'Deviatore', icon: '⑂', color: '#f472b6',
    help: 'Nodo di trasferimento: il nastro di ingresso ha questo componente come "successivo". Il valore del tag posizione sceglie l\'uscita; durante la commutazione le unità attendono in fondo al nastro.',
    fields: [['position', 'tag', 'Posizione (attuatore)'], ['outputs', 'outputs', 'Uscite'], ['switch_s', 'num', 'Tempo di commutazione (s)', '0,4'], ['fb_tag', 'stag', 'Posizione raggiunta (sensore)']],
  },
  source: {
    label: 'Sorgente (ingresso unità)', icon: '⊕', color: '#4ade80',
    help: 'Genera unità all\'inizio del nastro, con intervallo casuale tra min e max, se c\'è spazio.',
    fields: [['on', 'comp:conveyor', 'Nastro'], ['interval_min', 'num', 'Intervallo min (s)', '2'], ['interval_max', 'num', 'Intervallo max (s)', '= min'],
      ['materials', 'text', 'Materiali (virgola; vuoto = tutti)'], ['barcodes', 'text', 'Barcode: elenco, o modello con {n} / {n:6}', 'UDC000001…'],
      ['enable', 'tag', 'Abilitazione (tag, opzionale)'], ['max_items', 'num', 'Numero massimo di unità']],
  },
  sink: {
    label: 'Pozzo (uscita unità)', icon: '⊖', color: '#94a3b8',
    help: 'Le unità che arrivano qui lasciano l\'impianto. Collegalo come "successivo" di un nastro.',
    fields: [['count_tag', 'stag', 'Contatore (sensore, opzionale)']],
  },
  stopper: {
    label: 'Fermo / attuatore', icon: '⊥', color: '#fb7185',
    help: 'Attuatore lineare con tempo di corsa e finecorsa. Fuori, blocca le unità in quella posizione del nastro.',
    fields: [['cmd', 'tag', 'Comando fuori (attuatore)'], ['invert', 'bool', 'Comando invertito'], ['time_s', 'num', 'Tempo di corsa (s)', '0,3'],
      ['fb_ext', 'stag', 'Finecorsa fuori (sensore)'], ['fb_ret', 'stag', 'Finecorsa dentro (sensore)'], ['on', 'comp:conveyor', 'Montato su'], ['at_m', 'num', 'Posizione (m)', 'vuoto = dal disegno']],
  },
  // ---------------------------------------------------------------- fluidi
  tank: {
    label: 'Serbatoio', icon: '▯', color: '#38bdf8', fluid: true,
    help: 'Nodo con volume: livello dal bilancio di massa (ingressi − uscite − perdite, troppo pieno), temperatura dalla miscelazione, dai riscaldatori e dalla dispersione. La pressione sul fondo (quota + livello) muove i travasi per gravità.',
    fields: [['volume_m3', 'num', 'Volume (m³)', '1'], ['height_m', 'num', 'Altezza utile (m)', '1,5'], ['elevation_m', 'num', 'Quota del fondo (m)', '0'],
      ['level_tag', 'stag', 'Livello (sensore)'], ['level_max', 'num', 'Valore a serbatoio pieno', '100 (= %)'], ['temp_tag', 'stag', 'Temperatura °C (sensore)'],
      ['pressure_tag', 'stag', 'Pressione sul fondo bar (sensore)'], ['init_level', 'num', 'Livello iniziale %', '0'], ['init_temp', 'num', 'Temperatura iniziale °C', 'ambiente'],
      ['loss_w_k', 'num', 'Dispersione termica (W/K)', '30']],
  },
  supply: {
    label: 'Alimentazione', icon: '⇥', color: '#4ade80', fluid: true,
    help: 'Sorgente a pressione costante (acquedotto, rete vapore, collettore di servizio), con la sua temperatura.',
    fields: [['pressure_bar', 'num', 'Pressione (bar)', '3'], ['temp_c', 'num', 'Temperatura (°C)', '15']],
  },
  drain: {
    label: 'Scarico', icon: '⇤', color: '#94a3b8', fluid: true,
    help: 'Uscita a pressione atmosferica (fogna, carico autobotte, impianto a valle).',
    fields: [],
  },
  junction: {
    label: 'Giunzione / collettore', icon: '⊕', color: '#e879f9', fluid: true,
    help: 'Nodo senza volume dove si incontrano più tubazioni (es. una pompa che alimenta più utenze): la sua pressione è quella per cui le portate entranti e uscenti si bilanciano.',
    fields: [],
  },
  valve: {
    label: 'Valvola', icon: '⋈', color: '#fbbf24', fluid: true,
    help: 'In serie su una tubazione (campo Elementi della tubazione). On/off con un comando bool, o modulante con l\'apertura 0–100 %. Kv = portata in m³/h a 1 bar di caduta, a valvola tutta aperta.',
    fields: [['cmd', 'tag', 'Comando apertura (attuatore bool)'], ['invert', 'bool', 'Normalmente aperta (comando = chiudi)'], ['analog', 'bool', 'Modulante'],
      ['position', 'tag', 'Apertura % (attuatore, se modulante)'], ['kv', 'num', 'Kv (m³/h)', '10'], ['time_s', 'num', 'Tempo di corsa (s)', '2'],
      ['fb_open', 'stag', 'Finecorsa aperta (sensore)'], ['fb_closed', 'stag', 'Finecorsa chiusa (sensore)'], ['fb_pos', 'stag', 'Posizione % (sensore)']],
  },
  pump: {
    label: 'Pompa', icon: '◔', color: '#34d399', fluid: true,
    help: 'In serie su una tubazione. Curva centrifuga: prevalenza H0 a portata nulla, zero alla portata massima; velocità variabile con il setpoint. Ferma blocca il ramo; segnala la marcia a secco se aspira da un serbatoio vuoto.',
    fields: [['run', 'tag', 'Marcia (attuatore)'], ['speed_sp', 'tag', 'Setpoint velocità (attuatore)'], ['sp_max', 'num', 'Valore setpoint = 100 %', '100'],
      ['head_m', 'num', 'Prevalenza a portata nulla (m)', '20'], ['q_max', 'num', 'Portata massima (m³/h)', '10'], ['fb_run', 'stag', 'In marcia (sensore)']],
  },
  pipe: {
    label: 'Tubazione', icon: '═', color: '#60a5fa', fluid: true,
    help: 'Ramo da un nodo a un altro (serbatoio, alimentazione, scarico, giunzione), con valvole e al più una pompa in serie. Disegnala con un componente Linea: nel runtime il tubo si anima quando c\'è portata.',
    fields: [['from', 'comp:tank,supply,drain,junction', 'Da'], ['to', 'comp:tank,supply,drain,junction', 'A'],
      ['elements', 'text', 'Elementi in serie (id valvole/pompa, virgola)'], ['kv', 'num', 'Kv della tubazione (m³/h)', '100'],
      ['flow_tag', 'stag', 'Portata m³/h (sensore)'], ['flow_scale', 'num', 'Scala portata', '1']],
  },
  heater: {
    label: 'Riscaldatore', icon: '♨', color: '#f97316', fluid: true,
    help: 'Potenza ceduta al contenuto del serbatoio (non scalda sotto il 2 % di livello). Con il setpoint fa da termostato locale con isteresi.',
    fields: [['on', 'comp:tank', 'Nel serbatoio'], ['cmd', 'tag', 'Comando (attuatore)'], ['power_kw', 'num', 'Potenza (kW)', '10'],
      ['setpoint_tag', 'tag', 'Setpoint termostato °C (opzionale)'], ['hysteresis', 'num', 'Isteresi (°C)', '1']],
  },
  motor: {
    label: 'Motore / agitatore', icon: '⟳', color: '#a3e635', fluid: true,
    help: 'Motore generico (agitatore, ventilatore) con rampa: velocità reale verso il setpoint.',
    fields: [['run', 'tag', 'Marcia (attuatore)'], ['speed_sp', 'tag', 'Setpoint velocità (attuatore)'], ['sp_max', 'num', 'Valore setpoint = velocità max', '100'],
      ['rpm', 'num', 'Velocità max (es. giri/min)', '100'], ['time_s', 'num', 'Rampa 0 → max (s)', '2'], ['speed_fb', 'stag', 'Velocità reale (sensore)']],
  },
};

/** Asse del componente: inizio, direzione, lunghezza in px. */
export function axis(c) {
  const left = c.props?.direction === 'left';
  const a = ((+c.rotation || 0) + (left ? 180 : 0)) * Math.PI / 180;
  const ux = Math.cos(a), uy = Math.sin(a), w = +c.w || 100, h = +c.h || 40;
  const cx = (+c.x || 0) + w / 2, cy = (+c.y || 0) + h / 2;
  return { x0: cx - ux * w / 2, y0: cy - uy * w / 2, x1: cx + ux * w / 2, y1: cy + uy * w / 2, ux, uy, w, h, cx, cy, deg: a * 180 / Math.PI };
}

/** Scala della vista (mm per px) e px per metro di un nastro. */
export const viewScale = (view, def = 10) => +(view?.physics?.scale_mm) || def;
export function convPpm(c, view, def) {
  const len = +c.physics?.length_m;
  return len > 0 ? (+c.w || 100) / len : 1000 / viewScale(view, def);
}

/** Nastro su cui è montato un componente (esplicito o il più vicino), e posizione in m. */
export function mountOf(c, view, def) {
  const convs = view.components.filter((x) => x.physics?.kind === 'conveyor');
  let conv = c.physics?.on ? convs.find((x) => x.id === c.physics.on) : null;
  const ax0 = axis(c);
  if (!conv) {
    let best = null;
    for (const x of convs) {
      const a = axis(x);
      const dx = ax0.cx - a.x0, dy = ax0.cy - a.y0;
      const s = dx * a.ux + dy * a.uy, d = Math.abs(-dx * a.uy + dy * a.ux);
      if (s < -10 || s > a.w + 10) continue;
      const lim = a.h / 2 + Math.min(ax0.w, ax0.h) / 2 + 30;
      if (d <= lim && (!best || d < best.d)) best = { d, x };
    }
    conv = best?.x;
  }
  if (!conv) return null;
  const a = axis(conv), ppm = convPpm(conv, view, def);
  let at = +c.physics?.at_m;
  if (!(c.physics?.at_m >= 0 && c.physics?.at_m !== '' && c.physics?.at_m !== null)) {
    at = Math.max(0, Math.min(a.w, (ax0.cx - a.x0) * a.ux + (ax0.cy - a.y0) * a.uy)) / ppm;
  }
  return { conv, at, x: a.x0 + a.ux * at * ppm, y: a.y0 + a.uy * at * ppm, a };
}

/** Annotazioni del layer fisico nel Designer (SVG). */
export function physOverlay(view, def = 10) {
  if (!view) return '';
  const comps = view.components.filter((c) => c.physics?.kind);
  const byId = Object.fromEntries(view.components.map((c) => [c.id, c]));
  let h = `<defs><marker id="phArr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#22d3ee"/></marker>
    <marker id="phLink" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0 0L10 5L0 10z" fill="#e879f9"/></marker></defs>`;
  const entry = (c) => {
    if (!c) return null;
    if (c.physics?.kind === 'conveyor') { const a = axis(c); return [a.x0, a.y0]; }
    const a = axis(c); return [a.cx, a.cy];
  };
  const link = (x0, y0, t, label = '', ref = '') => {
    const p = entry(t);
    if (!p) {
      // collegamento verso un'altra vista (vista/id)
      return ref.includes('/') ? `<text x="${x0 + 6}" y="${y0 - 6}" fill="#e879f9" font-size="12" font-weight="600">↗ ${esc(ref)}${label ? ` (${esc(label)})` : ''}</text>` : '';
    }
    return `<line x1="${x0}" y1="${y0}" x2="${p[0]}" y2="${p[1]}" stroke="#e879f9" stroke-width="2" stroke-dasharray="5 4" marker-end="url(#phLink)"/>` +
      (label ? `<text x="${(x0 + p[0]) / 2 + 4}" y="${(y0 + p[1]) / 2 - 4}" fill="#e879f9" font-size="11" font-weight="600">${esc(label)}</text>` : '');
  };
  for (const c of comps) {
    const p = c.physics, k = KINDS[p.kind];
    if (!k) continue;
    if (p.kind === 'conveyor') {
      const a = axis(c), ppm = convPpm(c, view, def), len = (a.w / ppm);
      h += `<line x1="${a.x0}" y1="${a.y0}" x2="${a.x1}" y2="${a.y1}" stroke="${k.color}" stroke-width="3" marker-end="url(#phArr)" opacity=".9"/>
        <text x="${a.cx}" y="${a.cy + a.h / 2 + 13}" fill="${k.color}" font-size="11" text-anchor="middle" transform="rotate(${((a.deg % 360) + 360) % 360 > 90 && ((a.deg % 360) + 360) % 360 < 270 ? a.deg + 180 : a.deg} ${a.cx} ${a.cy})">${len.toFixed(2)} m · ${(+p.speed || 0.5)} m/s${p.run ? ' · ' + esc(p.run) : ' · ⚠ marcia?'}</text>`;
      if (p.next) h += link(a.x1, a.y1, byId[p.next], '', p.next);
      else if (p.sink) h += `<text x="${a.x1 + 4}" y="${a.y1 + 4}" fill="#94a3b8" font-size="14">⊖</text>`;
    } else if (p.kind === 'diverter') {
      const a = axis(c);
      h += `<circle cx="${a.cx}" cy="${a.cy}" r="9" fill="none" stroke="${k.color}" stroke-width="2.5"/>`;
      for (const o of p.outputs || []) h += link(a.cx, a.cy, byId[o.next], o.value, o.next);
    } else if (p.kind === 'sink') {
      const a = axis(c);
      h += `<text x="${a.cx}" y="${a.cy + 6}" fill="${k.color}" font-size="18" text-anchor="middle">⊖</text>`;
    } else if (k.fluid) {
      h += fluidMark(c, p, k, byId);
    } else {
      const m = mountOf(c, view, def), a = axis(c);
      if (!m) { h += `<text x="${a.cx}" y="${a.cy}" fill="#ef4444" font-size="12" text-anchor="middle">⚠ nessun nastro</text>`; continue; }
      const nx = -m.a.uy * (m.a.h / 2 + 6), ny = m.a.ux * (m.a.h / 2 + 6);
      if (p.kind === 'source') {
        h += `<circle cx="${m.a.x0}" cy="${m.a.y0}" r="8" fill="${k.color}" opacity=".85"/><text x="${m.a.x0}" y="${m.a.y0 + 4}" font-size="12" text-anchor="middle" fill="#0f172a">+</text>`;
      } else {
        h += `<line x1="${m.x - nx}" y1="${m.y - ny}" x2="${m.x + nx}" y2="${m.y + ny}" stroke="${k.color}" stroke-width="3"/>
          <line x1="${a.cx}" y1="${a.cy}" x2="${m.x}" y2="${m.y}" stroke="${k.color}" stroke-width="1" stroke-dasharray="2 3" opacity=".7"/>
          <text x="${m.x + nx + 3}" y="${m.y + ny + 11}" fill="${k.color}" font-size="10">${k.icon} ${m.at.toFixed(2)} m</text>`;
      }
    }
  }
  return h;
}

/** Annotazione di un componente dei fluidi. */
function fluidMark(c, p, k, byId) {
  const x = +c.x || 0, y = +c.y || 0, w = +c.w || 40, hh = +c.h || 40;
  const tag = (t, yy) => `<text x="${x + w / 2}" y="${yy}" fill="${k.color}" font-size="10" text-anchor="middle">${esc(t)}</text>`;
  const box = `<rect x="${x - 3}" y="${y - 3}" width="${w + 6}" height="${hh + 6}" fill="none" stroke="${k.color}" stroke-width="1.5" stroke-dasharray="4 3" rx="4"/>`;
  const center = (id) => { const t = byId[id]; return t ? [(+t.x || 0) + (+t.w || 0) / 2, (+t.y || 0) + (+t.h || 0) / 2] : null; };
  switch (p.kind) {
    case 'tank': return box + tag(`${k.icon} ${p.volume_m3 || 1} m³${p.elevation_m ? ` · quota ${p.elevation_m} m` : ''}`, y - 8);
    case 'pipe': {
      const a = center(p.from), b = center(p.to);
      const ax = axis(c);
      let h = `<line x1="${ax.x0}" y1="${ax.y0}" x2="${ax.x1}" y2="${ax.y1}" stroke="${k.color}" stroke-width="2" stroke-dasharray="2 3"/>`;
      if (a && b) h += `<line x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}" stroke="${k.color}" stroke-width="1.5" stroke-dasharray="6 4" marker-end="url(#phLink)" opacity=".7"/>`;
      return h + `<text x="${ax.cx + 6}" y="${ax.cy - 6}" fill="${k.color}" font-size="10">${esc(p.from || '?')} → ${esc(p.to || '?')}${p.elements ? ` · ${esc(p.elements)}` : ''}</text>`;
    }
    case 'heater': {
      const t = center(p.on);
      return box + tag(`${k.icon} ${p.power_kw || 10} kW`, y - 8) + (t ? `<line x1="${x + w / 2}" y1="${y + hh / 2}" x2="${t[0]}" y2="${t[1]}" stroke="${k.color}" stroke-dasharray="3 3"/>` : '');
    }
    case 'pump': return box + tag(`${k.icon} ${p.head_m || 20} m · ${p.q_max || 10} m³/h`, y - 8);
    case 'valve': return box + tag(`${k.icon} Kv ${p.kv || 10}${p.analog ? ' mod.' : ''}`, y - 8);
    default: return box + tag(`${k.icon} ${k.label}`, y - 8);
  }
}

/** Livello delle unità di carico in movimento sopra una vista (runtime). */
export class PhysLayer {
  constructor(svg) {
    this.g = document.createElementNS(NS, 'g');
    this.g.setAttribute('class', 'phys-items');
    this.g.style.pointerEvents = 'none';
    svg.appendChild(this.g);
    this.els = new Map();
  }
  /** items: [id, x, y, angolo, lunghezza px, larghezza px, colore, incastrata, barcode] */
  set(items) {
    const seen = new Set();
    for (const [id, x, y, a, l, w, col, jam, code] of items || []) {
      seen.add(id);
      let el = this.els.get(id);
      if (!el) {
        el = document.createElementNS(NS, 'g');
        el.innerHTML = `<rect rx="2" stroke="#1f2937" stroke-width="1"/><line stroke="#0f172a" stroke-width="1" opacity=".35"/><title></title>`;
        this.g.appendChild(el);
        this.els.set(id, el);
      }
      const r = el.firstChild, ln = el.children[1];
      r.setAttribute('x', -l / 2); r.setAttribute('y', -w / 2); r.setAttribute('width', l); r.setAttribute('height', w);
      r.setAttribute('fill', col); r.setAttribute('stroke', jam ? '#ef4444' : '#1f2937'); r.setAttribute('stroke-width', jam ? 3 : 1);
      ln.setAttribute('x1', -l / 2 + 2); ln.setAttribute('x2', l / 2 - 2); ln.setAttribute('y1', 0); ln.setAttribute('y2', 0);
      el.lastChild.textContent = `${code || '(senza etichetta)'} · unità ${id}${jam ? ' · INCASTRATA' : ''}`;
      el.setAttribute('transform', `translate(${x} ${y}) rotate(${a})`);
    }
    for (const [id, el] of this.els) if (!seen.has(id)) { el.remove(); this.els.delete(id); }
  }
  clear() { this.set([]); this.flows([]); }

  /** Tubazioni con portata: classe "phys-flow" sul componente (tubo animato). */
  flows(list) {
    const host = this.g.ownerSVGElement || this.g.parentNode;
    // il tubo e i suoi tratti di disegno (id_seg1, id_seg2…)
    const els = new Set();
    for (const f of list || []) {
      const id = CSS.escape(String(f[0]));
      host.querySelectorAll(`g[data-id="${id}"], g[data-id^="${id}_seg"]`).forEach((el) => els.add(el));
    }
    for (const el of host.querySelectorAll('g.phys-flow')) if (!els.has(el)) el.classList.remove('phys-flow');
    for (const el of els) el.classList.add('phys-flow');
  }
}
