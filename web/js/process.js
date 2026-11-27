// Runtime: sequenze di processo, ricette e audit trail.
import { api, esc, fmtTs, toast } from './core.js';
import { modal } from './forms.js';
import { can } from './auth.js';
import { helpBtn } from './markdown.js';

const ST = {
  idle: ['inattiva', ''], running: ['in esecuzione', 'ok'], paused: ['in pausa', 'warn'],
  done: ['completata', 'info'], stopped: ['arrestata', 'warn'], error: ['in errore', 'err'],
};
const dur = (ms) => {
  const s = Math.max(0, ms / 1000);
  return s < 60 ? `${s.toFixed(1)} s` : s < 3600 ? `${Math.floor(s / 60)} min ${Math.round(s % 60)} s` : `${(s / 3600).toFixed(1)} h`;
};
const valStr = (v) => (v === null || v === undefined ? '—' : typeof v === 'boolean' ? (v ? 'vero' : 'falso') : typeof v === 'number' ? (Number.isInteger(v) ? v : +v.toFixed(3)) : String(v));
const U = (s, d) => String(s ?? '').replaceAll('{unit}', d?.unit || '{unit}');
const condText = (c) => `${c.tag === '@elapsed' ? 'tempo nel passo' : c.tag === '@confirm' ? 'conferma operatore' : c.tag} ${c.op} ${typeof c.value === 'string' && c.value.startsWith('tag:') ? c.value.slice(4) : valStr(c.value)}${c.tag === '@elapsed' ? ' s' : ''}`;
const actText = (a) => ({
  write: () => `${a.tag} ← ${valStr(a.value ?? true)}`, recipe: () => `carica ricetta ${a.recipe_type}: ${a.recipe}`,
  script: () => `script ${a.fn}()`, log: () => `log: ${a.text}`,
}[a.kind]?.() || a.kind);

// ================================================================ sequenze
export async function showSequences(page, params, setTimer) {
  let sel = params.seq || '';
  page.innerHTML = `<div class="proc"><div class="row" style="margin-bottom:10px"><h2 class="grow" style="margin:0">Sequenze di processo ${helpBtn('runtime#sequenze')}</h2>
    <a href="#/process?tab=recipes"><button>🧪 Ricette</button></a></div>
    <div class="seq-grid" id="sqCards"></div><div id="sqDetail"></div></div>`;
  const op = can('seq.operate');
  const cmd = async (name, c, body = {}) => {
    try { await api.post(`/api/seq/${encodeURIComponent(name)}/${c}`, body); draw(); } catch (e) { toast(e.message, 'error'); }
  };
  async function startDialog(d) {
    const keys = Object.keys(d.params || {});
    const recipes = keys.includes('recipe') ? await api.get('/api/recipes').catch(() => []) : [];
    const rtype = d.steps.flatMap((s) => s.actions || []).find((a) => a.kind === 'recipe')?.recipe_type;
    const rlist = recipes.find((r) => r.type.name === rtype)?.recipes || [];
    modal(`Avvio sequenza ${d.name}`, `${d.description ? `<p class="hint">${esc(d.description)}</p>` : ''}
      ${keys.map((k) => `<div class="field"><label>${esc(k)}</label>${k === 'recipe' && rlist.length
        ? `<select data-p="${esc(k)}">${rlist.map((r) => `<option ${r.name === d.params[k] ? 'selected' : ''}>${esc(r.name)}</option>`).join('')}</select>`
        : `<input data-p="${esc(k)}" value="${esc(d.params[k] || '')}">`}</div>`).join('')}
      <div class="field"><label>Dal passo</label><select id="stStep">${d.steps.map((s, i) => `<option value="${i ? esc(s.name) : ''}">${i + 1}. ${esc(s.name)}</option>`).join('')}</select></div>`,
    [['Annulla', '', null], ['▶ Avvia', 'primary', async (b) => {
      const p = Object.fromEntries([...b.querySelectorAll('[data-p]')].map((x) => [x.dataset.p, x.value]));
      await cmd(d.name, 'start', { params: p, step: b.querySelector('#stStep').value || null });
    }]]);
  }

  async function draw() {
    let list;
    try { list = await api.get('/api/seq'); } catch (e) { page.querySelector('#sqCards').innerHTML = `<p class="err">${esc(e.message)}</p>`; return; }
    const cards = page.querySelector('#sqCards');
    if (!cards) return;
    if (!list.length) { cards.innerHTML = '<p class="muted">Nessuna sequenza configurata (Designer → ⚗ Processo).</p>'; return; }
    if (!sel || !list.some((x) => x.def.name === sel)) sel = list[0].def.name;
    cards.innerHTML = list.map(({ def: d, state: s }) => {
      const [sl, sc] = ST[s?.status || 'idle'];
      const active = s && ['running', 'paused'].includes(s.status);
      const waitConfirm = active && d.steps[s.step]?.transitions?.some((t) => t.when?.some((c) => c.tag === '@confirm'));
      return `<div class="card seq-card ${d.name === sel ? 'sel' : ''} st-${s?.status}" data-seq="${esc(d.name)}">
        <div class="row"><b class="grow">${esc(d.name)}</b><span class="chip ${sc}">${sl}</span></div>
        <div class="muted" style="font-size:12.5px">${esc(d.description || '')}${d.unit ? ` · unità <b>${esc(d.unit)}</b>` : ''}</div>
        <div class="seq-now">${active ? `Passo <b>${(s.step ?? 0) + 1}/${d.steps.length} · ${esc(s.step_name)}</b> · ${dur(s.elapsed_s * 1000)}` : s?.message ? esc(s.message) : '&nbsp;'}</div>
        ${active ? `<div class="seq-prog"><span style="width:${(((s.step ?? 0) + 1) / d.steps.length) * 100}%"></span></div>` : ''}
        <div class="row seq-btns" style="flex-wrap:wrap;gap:4px">
          ${!active ? `<button class="small primary" data-c="start" ${op ? '' : 'disabled'}>▶ Avvia</button>` : ''}
          ${s?.status === 'running' ? `<button class="small" data-c="pause" ${op ? '' : 'disabled'}>❚❚ Pausa</button>` : ''}
          ${s?.status === 'paused' ? `<button class="small primary" data-c="resume" ${op ? '' : 'disabled'}>▶ Riprendi</button>` : ''}
          ${waitConfirm ? `<button class="small primary" data-c="confirm" ${op ? '' : 'disabled'}>✓ Conferma</button>` : ''}
          ${active ? `<button class="small" data-c="next" ${op ? '' : 'disabled'} title="Passa al passo successivo senza attendere la transizione">⏭ Avanti</button>
            <button class="small danger" data-c="stop" ${op ? '' : 'disabled'}>■ Arresta</button>` : ''}
          ${!active && s && s.status !== 'idle' ? `<button class="small" data-c="reset" ${op ? '' : 'disabled'}>↺ Reset</button>` : ''}
        </div></div>`;
    }).join('');
    cards.querySelectorAll('[data-seq]').forEach((c) => (c.onclick = (e) => {
      const name = c.dataset.seq;
      const b = e.target.closest('[data-c]');
      if (b) {
        const d = list.find((x) => x.def.name === name).def;
        if (b.dataset.c === 'start') return startDialog(d);
        if (b.dataset.c === 'stop' && !confirm(`Arrestare la sequenza ${name}? Vengono eseguite le azioni di arresto.`)) return;
        if (b.dataset.c === 'next' && !confirm('Forzare il passaggio al passo successivo?')) return;
        return cmd(name, b.dataset.c);
      }
      sel = name; draw();
    }));
    drawDetail(list.find((x) => x.def.name === sel));
  }

  async function drawDetail({ def: d, state: s }) {
    const box = page.querySelector('#sqDetail');
    const runs = await api.get(`/api/seq/${encodeURIComponent(d.name)}/runs`).catch(() => []);
    const lastIdx = (name) => s?.history ? [...s.history].reverse().find((h) => h.step === name) : null;
    const active = s && ['running', 'paused'].includes(s.status);
    box.innerHTML = `<div class="grid2" style="grid-template-columns:minmax(380px,1fr) minmax(380px,1fr)">
      <div class="card"><h3>${esc(d.name)} – diagramma dei passi</h3><div class="sfc">
        ${d.steps.map((st, i) => {
          const h = lastIdx(st.name);
          const cur = active && s.step === i;
          const cls = cur ? 'cur' : h ? 'past' : '';
          const ut = (x) => U(x, d);
          return `<div class="sfc-step ${cls}"><div class="sfc-box"><span class="sfc-n">${i + 1}</span><b>${esc(st.name)}</b>
              ${cur ? `<span class="chip ok">${dur(s.elapsed_s * 1000)}${st.timeout_s ? ` / ${st.timeout_s} s` : ''}</span>` : h?.exit ? `<span class="muted">${dur(h.exit - h.enter)}</span>` : ''}
              ${op && active && !cur ? `<button class="small" data-goto="${esc(st.name)}" title="Vai a questo passo">↪</button>` : ''}
              ${st.description ? `<div class="muted">${esc(st.description)}</div>` : ''}
              ${(st.actions || []).map((a) => `<div class="sfc-act">⚡ ${esc(ut(actText(a)))}</div>`).join('')}</div>
            ${(st.transitions || []).map((t) => `<div class="sfc-tr"><span class="sfc-bar"></span>${t.label ? `<b>${esc(t.label)}</b> · ` : ''}${(t.when || []).map(condText).map(ut).map(esc).join(t.mode === 'any' ? ' <i>oppure</i> ' : ' <i>e</i> ') || 'subito'}
              → <b>${esc(t.next || (i + 1 < d.steps.length ? d.steps[i + 1].name : 'FINE'))}</b></div>`).join('')}
            ${st.timeout_s ? `<div class="sfc-tr to">⏱ timeout ${st.timeout_s} s → ${esc(st.on_timeout || 'errore')}</div>` : ''}</div>`;
        }).join('')}<div class="sfc-end">FINE</div></div></div>
      <div class="card"><h3>Esecuzione corrente</h3>
        ${s?.started ? `<p>Avvio ${fmtTs(s.started)} da <b>${esc(s.user || '—')}</b>${Object.keys(s.params || {}).length ? ` · parametri ${esc(JSON.stringify(s.params))}` : ''}</p>` : '<p class="muted">Mai avviata</p>'}
        ${s?.history?.length ? `<table><thead><tr><th>Passo</th><th>Ingresso</th><th>Durata</th><th>Uscita</th></tr></thead><tbody>${s.history.map((h) => `<tr><td>${esc(h.step)}</td><td>${new Date(h.enter).toLocaleTimeString()}</td>
          <td>${h.exit ? dur(h.exit - h.enter) : '…'}</td><td class="muted">${esc(h.reason || '')}</td></tr>`).join('')}</tbody></table>` : ''}
        <h3 style="margin-top:16px">Ultime esecuzioni</h3>
        ${runs.length ? `<table><thead><tr><th>Avvio</th><th>Esito</th><th>Durata</th><th>Utente</th><th>Messaggio</th></tr></thead><tbody>${runs.map((r) => `<tr><td>${fmtTs(r.started)}</td>
          <td><span class="chip ${ST[r.status]?.[1] || ''}">${ST[r.status]?.[0] || r.status}</span></td><td>${r.finished ? dur(r.finished - r.started) : ''}</td><td>${esc(r.user)}</td><td class="muted">${esc(r.message)}</td></tr>`).join('')}</tbody></table>` : '<p class="muted">Nessuna</p>'}
      </div></div>`;
    box.querySelectorAll('[data-goto]').forEach((b) => (b.onclick = () => { if (confirm(`Andare al passo ${b.dataset.goto}?`)) cmd(d.name, 'goto', { step: b.dataset.goto }); }));
  }

  await draw();
  setTimer(draw, 1000);
}

// ================================================================ ricette
export async function showRecipes(page, params, setTimer) {
  let types = await api.get('/api/recipes');
  page.innerHTML = `<div class="proc"><div class="row" style="margin-bottom:10px"><h2 class="grow" style="margin:0">Ricette ${helpBtn('runtime#ricette')}</h2>
    <a href="#/process"><button>🔁 Sequenze</button></a></div><div id="rcBody"></div></div>`;
  const body = page.querySelector('#rcBody');
  if (!types.length) { body.innerHTML = '<p class="muted">Nessun tipo di ricetta configurato (Designer → ⚗ Processo).</p>'; return; }
  let tname = params.type && types.some((t) => t.type.name === params.type) ? params.type : types[0].type.name;
  let rname = params.recipe || '';
  let unit = '';
  let cmp = null, draft = null;
  const canEdit = can('recipe.edit'), canLoad = can('recipe.load');

  async function reload() { types = await api.get('/api/recipes'); }
  function draw() {
    const T = types.find((t) => t.type.name === tname);
    const def = T.type;
    if (!unit && def.units?.length) unit = def.units[0];
    if (rname && !T.recipes.some((r) => r.name === rname) && !draft) rname = '';
    const r = draft || T.recipes.find((x) => x.name === rname);
    body.innerHTML = `<div class="grid2" style="grid-template-columns:minmax(300px,0.8fr) minmax(420px,1.6fr)">
      <div class="card"><div class="row"><select id="rcType">${types.map((t) => `<option ${t.type.name === tname ? 'selected' : ''}>${esc(t.type.name)}</option>`).join('')}</select>
        <span class="grow"></span><button class="small primary" id="rcNew" ${canEdit ? '' : 'disabled'}>+ Nuova</button></div>
        <p class="hint">${esc(def.description || '')}</p>
        ${def.units?.length ? `<h4>Ricetta attiva per unità</h4><div class="row" style="flex-wrap:wrap;gap:6px">${def.units.map((u) => { const a = T.active[u]; return `<span class="chip ${a?.ok === false ? 'err' : a ? 'ok' : ''}" title="${a ? `v${a.version} · ${fmtTs(a.ts)} · ${esc(a.by)}` : 'nessuna'}">${esc(u)}: <b>${esc(a?.recipe || '—')}</b></span>`; }).join('')}</div>` : ''}
        <table style="margin-top:10px"><thead><tr><th>Ricetta</th><th>Ver.</th><th>Modificata</th></tr></thead><tbody>
        ${T.recipes.map((x) => `<tr data-r="${esc(x.name)}" class="${x.name === rname && !draft ? 'sel' : ''}" style="cursor:pointer"><td><b>${esc(x.name)}</b><div class="muted" style="font-size:12px">${esc(x.description || '')}</div></td>
          <td>${x.version}</td><td class="muted" style="font-size:12px">${fmtTs(x.modified)}<br>${esc(x.modified_by)}</td></tr>`).join('') || '<tr><td colspan="3" class="muted">Nessuna ricetta</td></tr>'}</tbody></table></div>
      <div class="card">${r ? `<div class="row" style="flex-wrap:wrap"><h3 class="grow" style="margin:0">${draft ? '<input id="rcName" placeholder="nome ricetta" style="width:200px">' : esc(r.name)} ${r.version ? `<span class="chip">v${r.version}</span>` : ''}</h3>
          ${def.units?.length ? `<label class="row">Unità <select id="rcUnit">${def.units.map((u) => `<option ${u === unit ? 'selected' : ''}>${esc(u)}</option>`).join('')}</select></label>` : ''}
          <button id="rcRead" ${canEdit ? '' : 'disabled'} title="Copia nei valori della ricetta quelli attuali sul PLC">⇩ Leggi dal PLC</button>
          <button id="rcCmp" title="Confronta i valori della ricetta con quelli sul PLC">⇄ Confronta</button></div>
          <div class="field" style="margin-top:8px"><label>Descrizione</label><input id="rcDesc" value="${esc(r.description || '')}" ${canEdit ? '' : 'disabled'}></div>
          <table class="rc-tbl"><thead><tr><th>Parametro</th><th>Valore</th><th>Limiti</th><th>Tag</th>${cmp ? '<th>Sul PLC</th>' : ''}</tr></thead><tbody>
          ${def.parameters.map((p) => { const c = cmp?.find((x) => x.param === p.name); const v = r.values?.[p.name] ?? p.default ?? '';
            return `<tr class="${c && !c.equal ? 'diff' : ''}"><td><b>${esc(p.name)}</b><div class="muted" style="font-size:12px">${esc(p.description || '')}</div></td>
            <td><input data-v="${esc(p.name)}" value="${esc(valStr(v) === '—' ? '' : v)}" ${canEdit ? '' : 'disabled'} style="width:110px"> ${esc(p.unit || '')}</td>
            <td class="muted">${p.min ?? '−∞'} … ${p.max ?? '+∞'}</td><td class="mono muted">${esc(p.tag.replace('{unit}', unit))}</td>
            ${cmp ? `<td class="mono">${esc(valStr(c?.plc))} ${c && !c.equal ? '⚠' : '✓'}</td>` : ''}</tr>`; }).join('')}</tbody></table>
          <div class="row" style="margin-top:12px;flex-wrap:wrap">
            <button class="primary" id="rcSave" ${canEdit ? '' : 'disabled'}>💾 Salva</button>
            ${draft ? '<button id="rcCancel">Annulla</button>' : `<button id="rcCopy" ${canEdit ? '' : 'disabled'}>⧉ Salva come…</button><button class="danger" id="rcDel" ${canEdit ? '' : 'disabled'}>Elimina</button>`}
            <span class="grow"></span>
            ${draft ? '' : `<button class="primary" id="rcLoad" ${canLoad ? '' : 'disabled'}>⬆ Carica${unit ? ` su ${esc(unit)}` : ' sul PLC'}</button>`}</div>`
        : '<p class="muted">Seleziona o crea una ricetta.</p>'}</div></div>`;
    body.querySelector('#rcType').onchange = (e) => { tname = e.target.value; rname = ''; unit = ''; draft = null; cmp = null; draw(); };
    body.querySelectorAll('[data-r]').forEach((tr) => (tr.onclick = () => { rname = tr.dataset.r; draft = null; cmp = null; draw(); }));
    body.querySelector('#rcNew').onclick = () => { draft = { name: '', description: '', values: Object.fromEntries(def.parameters.map((p) => [p.name, p.default ?? ''])) }; cmp = null; draw(); };
    if (!r) return;
    const values = () => Object.fromEntries([...body.querySelectorAll('[data-v]')].map((i) => {
      const t = i.value.trim();
      return [i.dataset.v, t === '' ? null : t === 'vero' || t === 'true' ? true : t === 'falso' || t === 'false' ? false : isNaN(+t) ? t : +t];
    }));
    body.querySelector('#rcUnit')?.addEventListener('change', (e) => { unit = e.target.value; cmp = null; draw(); });
    body.querySelector('#rcCancel')?.addEventListener('click', () => { draft = null; draw(); });
    const save = async (name) => {
      if (!name) return toast('Nome della ricetta mancante', 'error');
      try {
        await api.put(`/api/recipes/${encodeURIComponent(tname)}/${encodeURIComponent(name)}`, { description: body.querySelector('#rcDesc').value, values: values() });
        toast(`Ricetta ${name} salvata`, 'ok');
        draft = null; rname = name; await reload(); draw();
      } catch (e) { toast(e.message, 'error'); }
    };
    body.querySelector('#rcSave').onclick = () => save(draft ? body.querySelector('#rcName').value.trim() : r.name);
    body.querySelector('#rcCopy')?.addEventListener('click', () => { const n = prompt('Nome della nuova ricetta:', `${r.name}_copia`); if (n) save(n.trim()); });
    body.querySelector('#rcDel')?.addEventListener('click', async () => {
      if (!confirm(`Eliminare la ricetta ${r.name}?`)) return;
      try { await api.del(`/api/recipes/${encodeURIComponent(tname)}/${encodeURIComponent(r.name)}`); rname = ''; await reload(); draw(); } catch (e) { toast(e.message, 'error'); }
    });
    body.querySelector('#rcRead').onclick = async () => {
      try {
        const cur = await api.get(`/api/recipes/${encodeURIComponent(tname)}/read?unit=${encodeURIComponent(unit)}`);
        body.querySelectorAll('[data-v]').forEach((i) => { const v = cur[i.dataset.v]; i.value = v === null || v === undefined ? '' : v; });
        toast('Valori letti dal PLC: salva per registrarli nella ricetta');
      } catch (e) { toast(e.message, 'error'); }
    };
    body.querySelector('#rcCmp').onclick = async () => {
      if (draft) return toast('Salva la ricetta prima del confronto');
      try { cmp = await api.get(`/api/recipes/${encodeURIComponent(tname)}/${encodeURIComponent(r.name)}/compare?unit=${encodeURIComponent(unit)}`); draw(); toast(cmp.every((c) => c.equal) ? 'Il PLC ha già i valori della ricetta' : `${cmp.filter((c) => !c.equal).length} parametri diversi sul PLC`); } catch (e) { toast(e.message, 'error'); }
    };
    body.querySelector('#rcLoad')?.addEventListener('click', async () => {
      if (!confirm(`Caricare la ricetta ${r.name}${unit ? ` sull'unità ${unit}` : ''}?\nI parametri vengono scritti sul PLC.`)) return;
      try {
        const res = await api.post(`/api/recipes/${encodeURIComponent(tname)}/${encodeURIComponent(r.name)}/load`, { unit });
        toast(res.ok ? `Ricetta ${r.name} caricata${unit ? ` su ${unit}` : ''}` : `Errori: ${res.results.filter((x) => !x.ok).map((x) => `${x.tag}: ${x.error}`).join('; ')}`, res.ok ? 'ok' : 'error');
        await reload(); cmp = null; draw();
      } catch (e) { toast(e.message, 'error'); }
    });
  }
  draw();
}

// ================================================================ audit
const CATS = { comandi: '🖱', configurazione: '⚙', sicurezza: '🔐', allarmi: '🔔', ricette: '🧪', sequenze: '🔁', wcs: '🏬', altro: '·' };
const ACTIONS = {
  write: 'Scrittura tag', ack: 'Riconoscimento allarmi', login: 'Accesso', logout: 'Uscita', login_failed: 'Accesso fallito', change_password: 'Cambio password',
  user_save: 'Utente salvato', user_delete: 'Utente eliminato', save_project: 'Progetto salvato', save_view: 'Vista salvata', delete_view: 'Vista eliminata',
  save_rules: 'Regole salvate', save_script: 'Script salvato', delete_script: 'Script eliminato', script_call: 'Script eseguito', recipe_save: 'Ricetta salvata',
  recipe_load: 'Ricetta caricata', recipe_create: 'Ricetta creata', recipe_delete: 'Ricetta eliminata', seq_start: 'Avvio sequenza', seq_stop: 'Arresto sequenza', seq_pause: 'Pausa sequenza',
  seq_resume: 'Ripresa sequenza', seq_reset: 'Reset sequenza', seq_next: 'Avanzamento manuale', seq_goto: 'Salto di passo', seq_confirm: 'Conferma operatore',
  audit_export: 'Esportazione audit',
};

export async function showAudit(page, params, live) {
  const day = 86400000;
  const iso = (t) => { const d = new Date(t - new Date().getTimezoneOffset() * 60000); return d.toISOString().slice(0, 16); };
  const f = { from: iso(Date.now() - 7 * day), to: '', user: '', category: params.category || '', action: '', q: '' };
  let offset = 0, last = null;
  const PAGE = 100;
  page.innerHTML = `<div class="card"><div class="row" style="flex-wrap:wrap;gap:8px">
      <h3 style="margin:0">Audit trail ${helpBtn('runtime#audit-trail')}</h3><span class="grow"></span>
      <label class="row">Dal <input type="datetime-local" id="aFrom" value="${f.from}"></label>
      <label class="row">al <input type="datetime-local" id="aTo"></label>
      <input id="aUser" placeholder="Utente" style="width:120px">
      <select id="aCat"><option value="">Tutte le categorie</option></select>
      <select id="aAct"><option value="">Tutte le azioni</option></select>
      <input id="aQ" placeholder="Cerca (tag, valore, dettaglio)…" style="width:200px">
      <button id="aCsv" title="Esporta in CSV (Excel) con i filtri correnti">⬇ CSV</button></div>
    <div class="row" style="margin:8px 0"><span id="aCount" class="muted grow"></span><label class="row muted"><input type="checkbox" id="aLive" checked> aggiornamento in tempo reale</label>
      <button class="small" id="aPrev">◀</button><span id="aPage" class="muted"></span><button class="small" id="aNext">▶</button></div>
    <div class="tbl-wrap"><table class="audit-tbl"><thead><tr><th>Data e ora</th><th>Utente</th><th>Categoria</th><th>Azione</th><th>Oggetto</th><th>Valore precedente</th><th>Valore nuovo</th><th>Dettaglio</th><th>Client</th></tr></thead><tbody id="aRows"></tbody></table></div></div>`;
  const q = () => {
    const p = new URLSearchParams({ limit: PAGE, offset });
    const t = (id) => page.querySelector(id).value;
    if (t('#aFrom')) p.set('from', new Date(t('#aFrom')).getTime());
    if (t('#aTo')) p.set('to', new Date(t('#aTo')).getTime());
    for (const [k, id] of [['user', '#aUser'], ['category', '#aCat'], ['action', '#aAct'], ['q', '#aQ']]) if (t(id).trim()) p.set(k, t(id).trim());
    return p;
  };
  const row = (r) => `<tr><td class="nowrap">${fmtTs(r.ts)}</td><td>${esc(r.user)}</td><td class="nowrap">${CATS[r.category] || ''} ${esc(r.category || '')}</td>
    <td>${esc(ACTIONS[r.action] || r.action)}</td><td class="mono">${esc(r.target || '')}</td>
    <td class="mono old">${esc(r.old_value ?? '')}</td><td class="mono new">${esc(r.new_value ?? '')}</td><td>${esc(r.detail || '')}</td><td class="muted mono">${esc(r.client || '')}</td></tr>`;
  async function draw() {
    let d;
    try { d = await api.get(`/api/audit?${q()}`); } catch (e) { page.querySelector('#aRows').innerHTML = `<tr><td colspan="9" class="err">${esc(e.message)}</td></tr>`; return; }
    last = d;
    const cat = page.querySelector('#aCat'), act = page.querySelector('#aAct');
    const keepC = cat.value, keepA = act.value;
    cat.innerHTML = '<option value="">Tutte le categorie</option>' + d.categories.map((c) => `<option value="${esc(c)}" ${c === keepC ? 'selected' : ''}>${CATS[c] || ''} ${esc(c)}</option>`).join('');
    act.innerHTML = '<option value="">Tutte le azioni</option>' + d.actions.map((a) => `<option value="${esc(a)}" ${a === keepA ? 'selected' : ''}>${esc(ACTIONS[a] || a)}</option>`).join('');
    page.querySelector('#aRows').innerHTML = d.rows.map(row).join('') || '<tr><td colspan="9" class="muted">Nessuna registrazione</td></tr>';
    page.querySelector('#aCount').textContent = `${d.total} registrazioni`;
    page.querySelector('#aPage').textContent = `${d.total ? offset + 1 : 0}–${Math.min(offset + PAGE, d.total)}`;
  }
  let t;
  page.querySelectorAll('input, select').forEach((el) => (el.oninput = () => { clearTimeout(t); t = setTimeout(() => { offset = 0; draw(); }, 300); }));
  page.querySelector('#aPrev').onclick = () => { offset = Math.max(0, offset - PAGE); draw(); };
  page.querySelector('#aNext').onclick = () => { if (last && offset + PAGE < last.total) { offset += PAGE; draw(); } };
  page.querySelector('#aCsv').onclick = () => { const p = q(); p.set('format', 'csv'); p.delete('limit'); p.delete('offset'); window.open(`/api/audit?${p}`, '_blank'); };
  // nuove righe in tempo reale (prima pagina, senza filtri testuali)
  live.on('audit', (d) => {
    if (!location.hash.startsWith('#/audit') || !page.querySelector('#aLive')?.checked || offset) return;
    const r = d.rec;
    const fc = page.querySelector('#aCat').value, fa = page.querySelector('#aAct').value;
    if ((fc && r.category !== fc) || (fa && r.action !== fa) || page.querySelector('#aQ').value || page.querySelector('#aUser').value) return;
    const tb = page.querySelector('#aRows');
    tb.insertAdjacentHTML('afterbegin', row(r).replace('<tr>', '<tr class="fresh">'));
    if (tb.children.length > PAGE) tb.lastElementChild.remove();
    if (last) page.querySelector('#aCount').textContent = `${++last.total} registrazioni`;
  });
  await draw();
}
