// Dati dal vivo di una fase o di un batch in esecuzione: tag letti e scritti (valori in
// tempo reale), passi attivi con tempi e timeout, valutazione delle condizioni di transizione.
import { api, esc, fmtTs } from './core.js';
import { dur, runInfo } from './sfc.js';

let defsCache = null;
async function tagDefs() {
  if (!defsCache) {
    defsCache = api.get('/api/tags').then((l) => Object.fromEntries(l.map((t) => [t.name, t]))).catch(() => ({}));
  }
  return defsCache;
}
let defs = {};
tagDefs().then((d) => (defs = d));

const num = (v) => (typeof v === 'number' ? v : typeof v === 'boolean' ? (v ? 1 : 0) : v !== '' && v !== null && !isNaN(+v) ? +v : null);
const fmt = (v, unit) => (v === undefined || v === null ? '—' : typeof v === 'boolean' ? (v ? 'vero' : 'falso') : typeof v === 'number' ? `${Math.round(v * 1000) / 1000}${unit ? ` ${unit}` : ''}` : String(v));

/** Tutti i tag citati da una procedura: scritti dalle azioni, letti dalle condizioni. */
export function phaseTags(steps, stateActs, sub) {
  const out = new Map();
  const add = (t, use) => {
    if (!t || t.startsWith('@')) return;
    const n = sub(t);
    const e = out.get(n) || new Set();
    e.add(use);
    out.set(n, e);
  };
  const walk = (list) => {
    for (const st of list || []) {
      for (const a of st.actions || []) if (a.kind === 'write') add(a.tag, 'scritto');
      for (const t of st.transitions || []) for (const c of t.when || []) {
        add(c.tag, 'condizione');
        if (typeof c.value === 'string' && c.value.startsWith('tag:')) add(c.value.slice(4), 'condizione');
      }
      for (const b of st.branches || []) walk(b);
    }
  };
  walk(steps);
  for (const a of stateActs || []) add(a.tag, 'azione di stato');
  return out;
}

/** Valuta una condizione con i valori correnti. */
export function evalCond(c, ctx) {
  const { live, sub, since, inst, children } = ctx;
  let cur;
  if (c.tag === '@elapsed') cur = since ? (Date.now() - since) / 1000 : null;
  else if (c.tag === '@confirm') cur = !!inst.confirm;
  else if (c.tag === '@phases_done') cur = children ? children.every((p) => p.state === 'complete') : null;
  else cur = live.value(sub(c.tag));
  let target = c.value;
  if (typeof target === 'string') {
    if (target.startsWith('tag:')) target = live.value(sub(target.slice(4)));
    else target = sub(target);
  }
  if (c.tag === '@confirm' || c.tag === '@phases_done') return { cur, target: true, ok: cur === true };
  const a = num(cur), b = num(target);
  const cmp = a !== null && b !== null ? [a, b] : [String(cur ?? ''), String(target ?? '')];
  const ok = cur === null || cur === undefined ? false : ({ '==': cmp[0] == cmp[1], '!=': cmp[0] != cmp[1], '>': cmp[0] > cmp[1], '>=': cmp[0] >= cmp[1], '<': cmp[0] < cmp[1], '<=': cmp[0] <= cmp[1] }[c.op] ?? false);
  return { cur, target, ok };
}

const stepAt = (steps, path) => {
  let list = steps;
  const segs = path.split('/');
  for (let k = 0; k < segs.length; k++) {
    const [si, bi] = segs[k].split('.').map(Number);
    const st = list?.[si - 1];
    if (k === segs.length - 1) return st;
    list = st?.branches?.[bi - 1];
  }
  return null;
};

/** Informazioni per il disegno SFC con le transizioni valutate dal vivo. */
export function liveRun(inst, live, children) {
  const run = runInfo(inst);
  run.evalTr = (t, path) => {
    const res = (t.when || []).map((c) => evalCond(c, { live, sub: run.subst, since: run.since[path], inst, children }));
    const ok = res.length ? (t.mode === 'any' ? res.some((r) => r.ok) : res.every((r) => r.ok)) : true;
    return { ok, vals: res.map((r) => fmt(r.cur)).join(' · ') };
  };
  return run;
}

/** Pannelli "Dati runtime" e "Tag della fase". */
export function liveHtml(inst, ctx) {
  const { live, sequences = [], children = [] } = ctx;
  const run = runInfo(inst);
  const sub = run.subst;
  const seq = inst.kind === 'phase' ? sequences.find((s) => s.name === inst.name) : null;
  const stateActs = seq ? ['hold', 'restart', 'stop', 'abort'].flatMap((k) => seq[`${k}_actions`] || []) : [];
  // passi attivi con tempi e condizioni
  const act = (inst.active || []).map((a) => {
    const st = stepAt(inst.steps || [], a.path) || {};
    const t = (Date.now() - a.since) / 1000;
    const trs = st.transitions || [];
    const conds = trs.map((tr, ti) => {
      const rs = (tr.when || []).map((c) => ({ c, r: evalCond(c, { live, sub, since: a.since, inst, children }) }));
      const ok = rs.length ? (tr.mode === 'any' ? rs.some((x) => x.r.ok) : rs.every((x) => x.r.ok)) : true;
      const tgt = tr.next ? (tr.next === 'END' ? 'FINE' : tr.next) : 'passo seguente';
      return `<div class="pl-tr ${ok ? 'ok' : ''}"><b>${ok ? '✓' : '…'} T${trs.length > 1 ? ti + 1 : ''}${tr.label ? ` ${esc(tr.label)}` : ''}</b> <span class="muted">→ ${esc(tgt)}${tr.mode === 'any' ? ' · almeno una' : ''}</span>
        ${rs.map(({ c, r }) => `<div class="pl-c ${r.ok ? 'ok' : 'no'}">${r.ok ? '✓' : '✗'} <code>${esc(c.tag === '@elapsed' ? 'tempo nel passo' : c.tag === '@confirm' ? 'conferma operatore' : c.tag === '@phases_done' ? 'fasi concluse' : sub(c.tag))}</code>
          ${c.tag === '@confirm' || c.tag === '@phases_done' ? '' : `${esc(c.op)} <b>${esc(fmt(r.target))}</b>`} <span class="muted">attuale</span> <b>${esc(fmt(r.cur, defs[sub(c.tag)]?.unit))}</b></div>`).join('') || '<div class="muted">nessuna condizione: sempre vera</div>'}</div>`;
    }).join('') || `<div class="muted">transizione implicita: ${st.branches?.length ? 'rami conclusi' : (st.actions || []).some((x) => x.kind === 'phase') ? 'fasi concluse' : 'subito'}</div>`;
    const to = st.timeout_s ? Math.max(0, st.timeout_s - t) : null;
    return `<div class="pl-step"><div class="row"><b>${esc(a.path.replaceAll('/', '.'))} ${esc(a.step)}</b><span class="grow"></span>
        <span class="chip ok">⧗ ${dur(Date.now() - a.since)}</span>${st.min_time_s ? `<span class="chip ${t >= st.min_time_s ? 'ok' : 'warn'}">minimo ${st.min_time_s} s</span>` : ''}
        ${to !== null ? `<span class="chip ${to < 10 ? 'err' : 'warn'}">timeout tra ${Math.round(to)} s → ${esc(st.on_timeout || 'errore')}</span>` : ''}</div>${conds}</div>`;
  }).join('') || `<p class="muted">Nessun passo attivo${inst.message ? ` · ${esc(inst.message)}` : ''}.</p>`;

  const info = [
    ['Stato', `<b>${esc(inst.state)}</b>${inst.message ? ` · ${esc(inst.message)}` : ''}`],
    [inst.kind === 'batch' ? 'Dispositivi' : 'Dispositivo', inst.kind === 'batch' ? Object.entries(inst.devices || {}).map(([k, v]) => `${esc(k)} = <b>${esc(v)}</b>`).join(', ') : `<b>${esc(inst.device || '—')}</b>`],
    ['Parametri', Object.entries(inst.params || {}).map(([k, v]) => `<code>${esc(k)} = ${esc(v)}</code>`).join(' ') || '—'],
    ['Avvio', `${fmtTs(inst.started)} · ${esc(inst.user || '')} · da ${dur(Date.now() - inst.started)}`],
    inst.owner ? ['Batch', `<a href="#/process?tab=batch&sel=${encodeURIComponent(inst.owner)}">${esc(inst.owner)}</a>`] : null,
    inst.held_since ? ['In hold da', dur(Date.now() - inst.held_since)] : null,
    inst.confirm ? ['Conferma operatore', '✓ data'] : null,
  ].filter(Boolean);

  const tags = phaseTags(inst.steps, stateActs, sub);
  // batch: tag delle fasi in corso, con la sostituzione dei rispettivi dispositivi e parametri
  for (const ph of children.filter((p) => ['running', 'held', 'error'].includes(p.state))) {
    const ps = sequences.find((x) => x.name === ph.name);
    const sa = ps ? ['hold', 'restart', 'stop', 'abort'].flatMap((k) => ps[`${k}_actions`] || []) : [];
    for (const [n, uses] of phaseTags(ph.steps, sa, runInfo(ph).subst)) {
      const e = tags.get(n) || new Set();
      uses.forEach((u) => e.add(`${ph.key}: ${u}`));
      tags.set(n, e);
    }
  }
  const rows = [...tags.entries()].map(([name, uses]) => {
    const s = live.tags.get(name);
    const d = defs[name];
    const q = s?.quality || (d ? '—' : 'sconosciuto');
    return `<tr><td><code>${esc(name)}</code>${d?.description ? `<div class="muted">${esc(d.description)}</div>` : ''}</td>
      <td class="r"><b>${esc(fmt(s?.value, d?.unit))}</b></td><td><span class="chip ${q === 'good' ? 'ok' : q === 'bad' ? 'err' : 'warn'}">${esc(q)}</span></td>
      <td class="muted">${[...uses].join(', ')}</td><td class="muted">${s?.ts ? new Date(s.ts).toLocaleTimeString() : ''}</td></tr>`;
  }).join('');

  return `<div class="grid2 phase-live">
    <div class="card"><h3>⏱ Dati runtime</h3><table class="pl-info">${info.map(([k, v]) => `<tr><th>${k}</th><td>${v}</td></tr>`).join('')}</table>
      <h4>Passi attivi e condizioni</h4>${act}
      ${inst.kind !== 'batch' && (inst.history || []).length ? `<h4>Ultimi passi</h4><table><tr><th>Passo</th><th>Ingresso</th><th>Durata</th><th>Uscita</th></tr>${inst.history.slice(-6).reverse().map((h) => `<tr><td>${esc(h.path)} ${esc(h.step)}</td><td>${new Date(h.enter).toLocaleTimeString()}</td><td>${h.exit ? dur(h.exit - h.enter) : '…'}</td><td class="muted">${esc(h.reason || '')}</td></tr>`).join('')}</table>` : ''}</div>
    <div class="card"><h3>🏷 Tag ${inst.kind === 'batch' ? 'della procedura e delle fasi in corso' : 'della fase'} (${tags.size})</h3>
      ${tags.size ? `<table class="pl-tags"><tr><th>Tag</th><th class="r">Valore</th><th>Qualità</th><th>Uso</th><th>Aggiornato</th></tr>${rows}</table>` : '<p class="muted">Nessun tag letto o scritto in questo momento.</p>'}
      ${children.length && inst.kind !== 'batch' ? `<h4>Fasi avviate</h4><table><tr><th>Fase</th><th>Stato</th><th>Passo</th></tr>${children.map((p) => `<tr><td><a href="#/process?tab=phases&sel=${encodeURIComponent(p.key)}">${esc(p.key)}</a></td><td>${esc(p.state)}</td><td>${(p.active || []).map((a) => `${esc(a.step)} <span class="muted">${dur(Date.now() - a.since)}</span>`).join(' ‖ ') || '—'}</td></tr>`).join('')}</table>` : ''}</div>
  </div>`;
}
