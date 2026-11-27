// SFC condiviso: diagramma (sola lettura, con passi attivi) ed editor di passi,
// rami paralleli, azioni e transizioni. Usato dalle sequenze (Designer) e dalle
// ricette (runtime).
import { esc } from './core.js';

const OPS = ['==', '!=', '>', '>=', '<', '<='];
export const STATES = {
  idle: ['inattiva', ''], running: ['in esecuzione', 'ok'], held: ['in hold', 'warn'], complete: ['completata', 'info'],
  stopped: ['arrestata', 'warn'], aborted: ['abortita', 'err'], error: ['in errore', 'err'],
};
export const dur = (ms) => {
  const s = Math.max(0, ms / 1000);
  return s < 60 ? `${s.toFixed(1)} s` : s < 3600 ? `${Math.floor(s / 60)} min ${Math.round(s % 60)} s` : `${(s / 3600).toFixed(1)} h`;
};
const valStr = (v) => (v === null || v === undefined ? '—' : typeof v === 'boolean' ? (v ? 'vero' : 'falso') : String(v));
const condText = (c) => `${c.tag === '@elapsed' ? 'tempo nel passo' : c.tag === '@confirm' ? 'conferma operatore' : c.tag === '@phases_done' ? 'fasi concluse' : c.tag} ${c.op} ${typeof c.value === 'string' && c.value.startsWith('tag:') ? c.value.slice(4) : valStr(c.value)}${c.tag === '@elapsed' ? ' s' : ''}`;
export const actText = (a) => ({
  write: () => `${a.tag} ← ${valStr(a.value ?? true)}`,
  phase: () => `▶ fase ${a.sequence} su ${a.device || '—'}${Object.keys(a.params || {}).length ? ` (${Object.entries(a.params).map(([k, v]) => `${k}=${v}`).join(', ')})` : ''}`,
  script: () => `script ${a.fn}()`, log: () => `log: ${a.text}`,
}[a.kind]?.() || a.kind);

// ================================================================ diagramma
/**
 * Diagramma SFC. `run` (facoltativo): { active: Set(path), since: {path: ts}, hist: {path: {enter, exit}}, subst(text) }.
 */
export function sfcDiagram(steps, run = {}, path = '') {
  const sub = run.subst || ((x) => x);
  return `<div class="sfc">${(steps || []).map((st, i) => {
    const p = path ? `${path}/${i + 1}` : String(i + 1);
    const act = run.active?.has(p);
    const h = run.hist?.[p];
    const cls = act ? 'cur' : h ? 'past' : '';
    const nextName = (t) => t.next === 'END' ? 'FINE' : t.next || (i + 1 < steps.length ? steps[i + 1].name : 'FINE');
    return `<div class="sfc-step ${cls}"><div class="sfc-box"><span class="sfc-n">${i + 1}</span><b>${esc(st.name)}</b>
        ${act ? `<span class="chip ok">${dur(Date.now() - (run.since?.[p] || Date.now()))}${st.timeout_s ? ` / ${st.timeout_s} s` : ''}</span>` : h?.exit ? `<span class="muted">${dur(h.exit - h.enter)}</span>` : ''}
        ${run.goto && !path ? `<button class="small" data-goto="${esc(st.name)}" title="Vai a questo passo">↪</button>` : ''}
        ${st.description ? `<div class="muted">${esc(st.description)}</div>` : ''}
        ${(st.actions || []).map((a) => `<div class="sfc-act ${a.kind === 'phase' ? 'ph' : ''}">⚡ ${esc(sub(actText(a)))}</div>`).join('')}</div>
      ${st.branches?.length ? `<div class="sfc-par"><div class="sfc-dbl"></div><div class="sfc-brs">${st.branches.map((b, k) => `<div class="sfc-br"><div class="sfc-brh">ramo ${k + 1}</div>${sfcDiagram(b, run, `${p}.${k + 1}`)}</div>`).join('')}</div><div class="sfc-dbl"></div></div>` : ''}
      ${(st.transitions || []).map((t) => `<div class="sfc-tr"><span class="sfc-bar"></span>${t.label ? `<b>${esc(t.label)}</b> · ` : ''}${(t.when || []).map((c) => esc(sub(condText(c)))).join(t.mode === 'any' ? ' <i>oppure</i> ' : ' <i>e</i> ') || (st.actions?.some((a) => a.kind === 'phase') ? 'fasi concluse' : 'subito')}
        → <b>${esc(nextName(t))}</b></div>`).join('') || `<div class="sfc-tr"><span class="sfc-bar"></span>${st.branches?.length ? 'tutti i rami conclusi' : st.actions?.some((a) => a.kind === 'phase') ? 'fasi concluse' : 'subito'} → <b>${esc(i + 1 < steps.length ? steps[i + 1].name : 'FINE')}</b></div>`}
      ${st.timeout_s ? `<div class="sfc-tr to">⏱ timeout ${st.timeout_s} s → ${esc(st.on_timeout || 'errore')}</div>` : ''}</div>`;
  }).join('')}${path ? '' : '<div class="sfc-end">FINE</div>'}</div>`;
}

/** Dati di esecuzione di un'istanza per il diagramma. */
export function runInfo(inst) {
  const active = new Set((inst?.active || []).map((a) => a.path));
  const since = Object.fromEntries((inst?.active || []).map((a) => [a.path, a.since]));
  const hist = {};
  for (const h of inst?.history || []) hist[h.path] = h;
  const map = { ...(inst?.params || {}), ...(inst?.devices || {}), unit: inst?.device };
  const subst = (t) => String(t).replace(/\{(\w+)\}/g, (m, k) => (map[k] !== undefined && map[k] !== '' ? map[k] : m));
  return { active, since, hist, subst };
}

// ================================================================ editor
/**
 * Editor di una rete SFC. `opts`: { mode: 'sequence'|'recipe', onChange, ctx: {
 *   sequences: [defs], functions: [nomi], aliases: () => [alias], params: () => [nomi parametri] } }
 */
export function sfcEditor(host, steps, opts) {
  const open = new Set(['1']);
  const kinds = opts.mode === 'recipe'
    ? [['phase', 'Avvia fase'], ['write', 'Scrivi tag'], ['script', 'Script Python'], ['log', 'Messaggio nel log']]
    : [['write', 'Scrivi tag'], ['script', 'Script Python'], ['log', 'Messaggio nel log']];
  const changed = () => opts.onChange?.();
  const parseVal = (v) => (v === 'true' ? true : v === 'false' ? false : v.trim() !== '' && !isNaN(+v) ? +v : v);
  const pick = (root, sel) => root.querySelectorAll(`:scope > ${sel}`);

  function actHtml(a) {
    const seq = opts.ctx.sequences?.find((s) => s.name === a.sequence);
    const aliases = opts.ctx.aliases?.() || [];
    const params = opts.ctx.params?.() || [];
    return `<div class="pr-row act-row">
      <select data-k="kind">${kinds.map(([k, l]) => `<option value="${k}" ${k === a.kind ? 'selected' : ''}>${l}</option>`).join('')}</select>
      ${a.kind === 'write' ? `<input data-k="tag" list="tagNames" value="${esc(a.tag || '')}" placeholder="{unit}.Tag"><span>←</span><input data-k="value" value="${esc(a.value === undefined ? 'true' : String(a.value))}" style="width:100px" placeholder="valore o {param}">`
      : a.kind === 'phase' ? `<select data-k="sequence">${(opts.ctx.sequences || []).map((s) => `<option ${s.name === a.sequence ? 'selected' : ''}>${esc(s.name)}</option>`).join('')}</select>
        <span>su</span><select data-k="device">${seq?.device_class ? '' : '<option value="">(nessun dispositivo)</option>'}${aliases.map((x) => `<option value="{${esc(x)}}" ${a.device === `{${x}}` ? 'selected' : ''}>{${esc(x)}}</option>`).join('')}</select>
        <div class="phase-params">${(seq?.parameters || []).map((p) => `<label class="chip">${esc(p.name)}<input data-pp="${esc(p.name)}" list="rcpParams" value="${esc(a.params?.[p.name] ?? '')}" placeholder="${esc(p.default ?? '')}" style="width:90px">${esc(p.unit || '')}</label>`).join('') || '<span class="muted">nessun parametro</span>'}</div>
        <datalist id="rcpParams">${params.map((x) => `<option value="{${esc(x)}}">`).join('')}</datalist>`
      : a.kind === 'script' ? `<select data-k="fn">${(opts.ctx.functions || []).map((f) => `<option ${f === a.fn ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select>`
      : `<input data-k="text" value="${esc(a.text || '')}" placeholder="testo, es. {unit} pronto" style="flex:1">`}
      <button class="small danger" data-adel>✕</button></div>`;
  }

  function stepsHtml(list, path) {
    const names = list.map((s) => s.name);
    const nextOpts = (v) => `<option value="" ${!v ? 'selected' : ''}>(passo seguente)</option>${names.map((n) => `<option ${n === v ? 'selected' : ''}>${esc(n)}</option>`).join('')}<option value="END" ${v === 'END' ? 'selected' : ''}>FINE ${path ? '(del ramo)' : ''}</option>`;
    return `<div class="sfc-ed">${list.map((st, i) => {
      const p = path ? `${path}/${i + 1}` : String(i + 1);
      const isOpen = open.has(p);
      return `<div class="pr-step ${isOpen ? 'open' : ''}" data-p="${p}">
        <div class="pr-head"><span class="sfc-n">${i + 1}</span><input data-sf="name" value="${esc(st.name)}" style="width:170px;font-weight:700">
          <span class="muted grow" style="font-size:12px">${(st.actions || []).length} azioni · ${(st.transitions || []).length} transizioni${st.branches?.length ? ` · ${st.branches.length} rami paralleli` : ''}${st.timeout_s ? ` · timeout ${st.timeout_s} s` : ''}</span>
          <button class="small" data-mv="-1" title="Su">↑</button><button class="small" data-mv="1" title="Giù">↓</button><button class="small" data-ins title="Inserisci un passo dopo">+</button>
          <button class="small danger" data-sdel title="Elimina passo">✕</button><button class="small" data-tg>${isOpen ? '▾' : '▸'}</button></div>
        ${isOpen ? `<div class="pr-body">
          <div class="row" style="flex-wrap:wrap;gap:10px"><label class="row">Descrizione <input data-sf="description" value="${esc(st.description || '')}" style="width:240px"></label>
            <label class="row" title="Tempo minimo prima di valutare le transizioni">T. minimo (s) <input type="number" data-sf="min_time_s" value="${st.min_time_s ?? ''}" style="width:70px"></label>
            <label class="row" title="vuoto = nessun timeout">Timeout (s) <input type="number" data-sf="timeout_s" value="${st.timeout_s ?? ''}" style="width:70px"></label>
            <label class="row">in timeout → <select data-sf="on_timeout"><option value="" ${!st.on_timeout ? 'selected' : ''}>errore + allarme</option>${names.map((n) => `<option ${n === st.on_timeout ? 'selected' : ''}>${esc(n)}</option>`).join('')}</select></label></div>
          <h5>⚡ Azioni all'ingresso</h5><div class="acts">${(st.actions || []).map(actHtml).join('')}</div><button class="small" data-aadd>+ Azione</button>
          <h5>⫴ Rami paralleli <span class="muted" style="text-transform:none;letter-spacing:0">partono insieme; il passo prosegue quando tutti sono in fondo</span></h5>
          <div class="pr-brs">${(st.branches || []).map((b, k) => `<div class="pr-br" data-b="${k}"><div class="row"><b>Ramo ${k + 1}</b><span class="grow"></span><button class="small danger" data-bdel="${k}">✕ ramo</button></div>${stepsHtml(b, `${p}.${k + 1}`)}</div>`).join('')}</div>
          <button class="small" data-badd>+ Ramo parallelo</button>
          <h5>➜ Transizioni (la prima vera vince; nessuna = ${opts.mode === 'recipe' ? 'fasi e rami conclusi' : 'subito / rami conclusi'})</h5>
          ${(st.transitions || []).map((t, ti) => `<div class="pr-tr" data-ti="${ti}">
            <div class="row" style="gap:6px;flex-wrap:wrap"><b>Se</b><select data-tf="mode"><option value="all" ${t.mode !== 'any' ? 'selected' : ''}>tutte</option><option value="any" ${t.mode === 'any' ? 'selected' : ''}>almeno una</option></select>
              <span>vai a</span><select data-tf="next">${nextOpts(t.next)}</select><input data-tf="label" value="${esc(t.label || '')}" placeholder="etichetta" style="width:150px">
              <span class="grow"></span><button class="small" data-cadd>+ Condizione</button><button class="small danger" data-tdel>✕</button></div>
            ${(t.when || []).map((c, ci) => `<div class="pr-row" data-ci="${ci}"><input data-cf="tag" list="seqTags" value="${esc(c.tag)}" placeholder="tag, @elapsed, @confirm, @phases_done">
              <select data-cf="op">${OPS.map((o) => `<option ${o === c.op ? 'selected' : ''}>${o}</option>`).join('')}</select>
              <input data-cf="value" value="${esc(String(c.value ?? ''))}" placeholder="valore, {param} o tag:Nome" style="width:170px"><button class="small danger" data-cdel>✕</button></div>`).join('') || '<div class="muted" style="font-size:12px;margin:4px 0">nessuna condizione</div>'}
          </div>`).join('')}<button class="small" data-tadd>+ Transizione</button>
        </div>` : ''}</div>`;
    }).join('')}<button class="small" data-sadd>+ Passo${path ? ' nel ramo' : ''}</button></div>`;
  }

  function wire(root, list, path) {
    root.querySelector(':scope > [data-sadd]').onclick = () => { list.push({ name: `Passo ${list.length + 1}` }); open.add(path ? `${path}/${list.length}` : String(list.length)); redraw(); };
    pick(root, '.pr-step').forEach((box) => {
      const i = [...box.parentNode.children].filter((x) => x.classList.contains('pr-step')).indexOf(box);
      const st = list[i], p = box.dataset.p;
      const head = box.querySelector(':scope > .pr-head');
      head.querySelector('[data-tg]').onclick = () => { open.has(p) ? open.delete(p) : open.add(p); redraw(); };
      head.querySelector('[data-mv="-1"]').onclick = () => { if (i > 0) { [list[i - 1], list[i]] = [list[i], list[i - 1]]; redraw(true); } };
      head.querySelector('[data-mv="1"]').onclick = () => { if (i < list.length - 1) { [list[i + 1], list[i]] = [list[i], list[i + 1]]; redraw(true); } };
      head.querySelector('[data-ins]').onclick = () => { list.splice(i + 1, 0, { name: `Passo ${list.length + 1}` }); redraw(true); };
      head.querySelector('[data-sdel]').onclick = () => { if (confirm(`Eliminare il passo ${st.name}?`)) { list.splice(i, 1); redraw(true); } };
      const sf = (el) => {
        const k = el.dataset.sf, old = st.name;
        if (['min_time_s', 'timeout_s'].includes(k)) { if (el.value === '' || +el.value === 0) delete st[k]; else st[k] = +el.value; }
        else if (el.value === '' && k !== 'name') delete st[k]; else st[k] = el.value;
        if (k === 'name' && old !== st.name) {
          for (const x of list) { for (const t of x.transitions || []) if (t.next === old) t.next = st.name; if (x.on_timeout === old) x.on_timeout = st.name; }
          return redraw(true);
        }
        changed();
      };
      head.querySelectorAll('[data-sf]').forEach((el) => (el.onchange = () => sf(el)));
      const body = box.querySelector(':scope > .pr-body');
      if (!body) return;
      body.querySelectorAll(':scope > .row [data-sf]').forEach((el) => (el.onchange = () => sf(el)));
      // azioni
      body.querySelector(':scope > [data-aadd]').onclick = () => { (st.actions ||= []).push(opts.mode === 'recipe' ? { kind: 'phase', sequence: opts.ctx.sequences?.[0]?.name || '', device: `{${opts.ctx.aliases?.()[0] || ''}}` } : { kind: 'write', tag: '', value: true }); redraw(); };
      body.querySelectorAll(':scope > .acts > .act-row').forEach((row, ai) => {
        const a = st.actions[ai];
        row.querySelectorAll('[data-k]').forEach((el) => (el.onchange = () => {
          const k = el.dataset.k;
          if (k === 'kind') { for (const x of Object.keys(a)) delete a[x]; a.kind = el.value; if (a.kind === 'phase') { a.sequence = opts.ctx.sequences?.[0]?.name || ''; a.device = `{${opts.ctx.aliases?.()[0] || ''}}`; } return redraw(); }
          if (k === 'sequence') { a.sequence = el.value; a.params = {}; return redraw(); }
          if (k === 'device' && !el.value) delete a.device; else a[k] = k === 'value' ? parseVal(el.value) : el.value;
          changed();
        }));
        row.querySelectorAll('[data-pp]').forEach((el) => (el.onchange = () => {
          a.params ||= {};
          if (el.value.trim()) a.params[el.dataset.pp] = el.value.trim(); else delete a.params[el.dataset.pp];
          if (!Object.keys(a.params).length) delete a.params;
          changed();
        }));
        row.querySelector('[data-adel]').onclick = () => { st.actions.splice(ai, 1); if (!st.actions.length) delete st.actions; redraw(); };
      });
      // rami
      body.querySelector(':scope > [data-badd]').onclick = () => { (st.branches ||= []).push([{ name: `R${st.branches.length + 1}.1` }]); open.add(`${p}.${st.branches.length}/1`); redraw(); };
      body.querySelectorAll(':scope > .pr-brs > .pr-br').forEach((bb) => {
        const k = +bb.dataset.b;
        bb.querySelector(`[data-bdel="${k}"]`).onclick = () => { if (confirm(`Eliminare il ramo ${k + 1}?`)) { st.branches.splice(k, 1); if (!st.branches.length) delete st.branches; redraw(); } };
        wire(bb.querySelector(':scope > .sfc-ed'), st.branches[k], `${p}.${k + 1}`);
      });
      // transizioni
      body.querySelector(':scope > [data-tadd]').onclick = () => { (st.transitions ||= []).push({ when: [{ tag: '', op: '==', value: true }] }); redraw(); };
      body.querySelectorAll(':scope > .pr-tr').forEach((tb) => {
        const t = st.transitions[+tb.dataset.ti];
        tb.querySelectorAll('[data-tf]').forEach((el) => (el.onchange = () => { const k = el.dataset.tf; if (el.value === '' || (k === 'mode' && el.value === 'all')) delete t[k]; else t[k] = el.value; changed(); }));
        tb.querySelector('[data-cadd]').onclick = () => { (t.when ||= []).push({ tag: '', op: '==', value: true }); redraw(); };
        tb.querySelector('[data-tdel]').onclick = () => { st.transitions.splice(+tb.dataset.ti, 1); if (!st.transitions.length) delete st.transitions; redraw(); };
        tb.querySelectorAll('[data-ci]').forEach((cr) => {
          const c = t.when[+cr.dataset.ci];
          cr.querySelectorAll('[data-cf]').forEach((el) => (el.onchange = () => { c[el.dataset.cf] = el.dataset.cf === 'value' && !el.value.startsWith('tag:') && !el.value.includes('{') ? parseVal(el.value) : el.value.trim(); changed(); }));
          cr.querySelector('[data-cdel]').onclick = () => { t.when.splice(+cr.dataset.ci, 1); if (!t.when.length) delete t.when; redraw(); };
        });
      });
    });
  }

  function redraw(collapse = false, silent = false) {
    if (collapse) open.clear();
    host.innerHTML = stepsHtml(steps, '') + `<datalist id="seqTags"><option value="@elapsed">secondi nel passo</option><option value="@confirm">conferma operatore</option>${opts.mode === 'recipe' ? '<option value="@phases_done">fasi del passo concluse</option>' : ''}</datalist>`;
    wire(host.querySelector('.sfc-ed'), steps, '');
    if (!silent) changed();
  }
  host.innerHTML = '';
  redraw(false, true);
  return { redraw };
}

/** Editor delle azioni di stato (hold, restart, stop, abort). */
export function stateActionsHtml(obj) {
  return ['hold', 'restart', 'stop', 'abort'].map((k) => `<div class="pr-state"><b>${{ hold: '❚❚ Hold', restart: '▶ Restart', stop: '■ Stop', abort: '⛔ Abort' }[k]}</b>
    <span class="muted" style="font-size:12px">${{ hold: 'messa in attesa sicura', restart: 'ripresa dopo hold o errore', stop: 'arresto controllato', abort: 'arresto immediato (vuoto = come stop)' }[k]}</span>
    <div data-sa="${k}">${(obj[`${k}_actions`] || []).map((a, i) => `<div class="pr-row" data-i="${i}"><input data-sk="tag" list="tagNames" value="${esc(a.tag || '')}" placeholder="{unit}.Tag"><span>←</span><input data-sk="value" value="${esc(String(a.value ?? ''))}" style="width:90px"><button class="small danger" data-sdel>✕</button></div>`).join('')}
    <button class="small" data-sadd>+ Scrittura</button></div></div>`).join('');
}

export function wireStateActions(root, obj, onChange) {
  root.querySelectorAll('[data-sa]').forEach((box) => {
    const k = `${box.dataset.sa}_actions`;
    box.querySelector('[data-sadd]').onclick = () => { (obj[k] ||= []).push({ kind: 'write', tag: '', value: false }); onChange(true); };
    box.querySelectorAll('[data-i]').forEach((row) => {
      const a = obj[k][+row.dataset.i];
      row.querySelectorAll('[data-sk]').forEach((el) => (el.onchange = () => { a[el.dataset.sk] = el.dataset.sk === 'value' ? (el.value === 'true' ? true : el.value === 'false' ? false : el.value !== '' && !isNaN(+el.value) ? +el.value : el.value) : el.value; onChange(false); }));
      row.querySelector('[data-sdel]').onclick = () => { obj[k].splice(+row.dataset.i, 1); if (!obj[k].length) delete obj[k]; onChange(true); };
    });
  });
}
