// Runtime: processo ISA-88 (batch, fasi, ricette, dispositivi, storico) e audit trail.
import { api, esc, fmtTs, toast } from './core.js';
import { modal } from './forms.js';
import { can } from './auth.js';
import { helpBtn } from './markdown.js';
import { STATES, dur, runInfo, stateActionsHtml, wireStateActions } from './sfc.js';
import { sfcFlow, sfcFlowView } from './sfcflow.js';
import { liveHtml, liveRun } from './phaselive.js';

const TABS = [['batch', '🧾 Batch'], ['phases', '🔁 Fasi'], ['recipes', '🧪 Ricette'], ['devices', '🏭 Dispositivi'], ['history', '📚 Storico']];
const chip = (s) => { const [l, c] = STATES[s] || [s, '']; return `<span class="chip ${c}">${l}</span>`; };
const activeTxt = (i) => (i.active || []).map((a) => `${esc(a.step)} <span class="muted">${dur(Date.now() - a.since)}</span>`).join(' ‖ ') || (i.message ? esc(i.message) : '');

/** Passo di una procedura dal percorso ("2.1/3" = passo 3 del ramo 1 del passo 2). */
function stepAt(steps, path) {
  const segs = path.split('/');
  let list = steps;
  for (let k = 0; k < segs.length; k++) {
    const [si, bi] = segs[k].split('.').map(Number);
    const st = list?.[si - 1];
    if (k === segs.length - 1) return st;
    list = st?.branches?.[bi - 1];
  }
  return null;
}
let PROC_STEPS = () => null;

/** Pulsanti di comando ISA-88 per un'istanza. */
function cmdButtons(i, allowed) {
  const d = allowed ? '' : 'disabled';
  const st = i.state;
  const steps = PROC_STEPS(i);
  const waitConfirm = ['running', 'held'].includes(st) && (i.active || []).some((a) => stepAt(steps || [], a.path)?.transitions?.some((t) => t.when?.some((c) => c.tag === '@confirm')));
  return [
    st === 'running' ? `<button class="small" data-cmd="hold" ${d}>❚❚ Hold</button>` : '',
    ['held', 'error'].includes(st) ? `<button class="small primary" data-cmd="restart" ${d}>▶ Restart</button>` : '',
    waitConfirm ? `<button class="small" data-cmd="confirm" ${d} title="Conferma operatore (@confirm)">✓ Conferma</button>` : '',
    ['running', 'held', 'error'].includes(st) ? `<button class="small" data-cmd="stop" ${d}>■ Stop</button><button class="small danger" data-cmd="abort" ${d}>⛔ Abort</button>` : '',
    ['complete', 'stopped', 'aborted', 'error'].includes(st) ? `<button class="small" data-cmd="reset" ${d}>↺ Reset</button>` : '',
  ].join('');
}

export async function showProcess(page, params, setTimer, live) {
  const tab = TABS.some(([k]) => k === params.tab) ? params.tab : 'batch';
  page.innerHTML = `<div class="proc"><div class="row" style="margin-bottom:10px;flex-wrap:wrap"><h2 style="margin:0 12px 0 0">Processo ${helpBtn('runtime#processo-batch-e-fasi')}</h2>
      <div class="subtabs" style="margin:0">${TABS.map(([k, l]) => `<button data-t="${k}" class="${k === tab ? 'on' : ''}">${l}</button>`).join('')}</div></div><div id="prBody"></div></div>`;
  page.querySelectorAll('[data-t]').forEach((b) => (b.onclick = () => (location.hash = `#/process?tab=${b.dataset.t}`)));
  const body = page.querySelector('#prBody');
  let sel = params.sel || '';
  let st = await api.get('/api/proc');
  PROC_STEPS = (i) => i.steps;
  const reload = async () => { st = await api.get('/api/proc'); };
  const cmd = async (key, c, extra = {}) => {
    if (c === 'abort' && !confirm(`Abort di ${key}? Arresto immediato (azioni di abort) e rilascio dei dispositivi.`)) return;
    if (c === 'stop' && !confirm(`Stop di ${key}? Vengono eseguite le azioni di stop.`)) return;
    try { await api.post(`/api/inst/${encodeURIComponent(key)}/${c}`, extra); await reload(); draw(); } catch (e) { toast(e.message, 'error'); }
  };
  const wireCmds = (root, key) => root.querySelectorAll('[data-cmd]').forEach((b) => (b.onclick = (e) => { e.stopPropagation(); cmd(key, b.dataset.cmd); }));

  // ------------------------------------------------------------ batch
  function drawBatch() {
    const batches = st.instances.filter((i) => i.kind === 'batch');
    const op = can('batch.operate');
    if (!batches.some((b) => b.key === sel)) sel = batches[0]?.key || '';
    body.innerHTML = `<div class="row" style="margin-bottom:10px"><span class="muted grow">${batches.length} batch attivi · ${st.devices.filter((d) => d.batch).length}/${st.devices.length} dispositivi allocati</span>
        <button class="primary" id="bNew" ${op && st.recipes.length ? '' : 'disabled'}>▶ Nuovo batch</button></div>
      <div class="seq-grid">${batches.map((b) => `<div class="card seq-card st-${b.state} ${b.key === sel ? 'sel' : ''}" data-k="${esc(b.key)}">
        <div class="row"><b class="grow">${esc(b.key)} · ${esc(b.name)}</b>${chip(b.state)}</div>
        <div class="muted" style="font-size:12.5px">v${b.version}${b.label ? ` · ${esc(b.label)}` : ''} · ${Object.entries(b.devices).map(([a, d]) => `${esc(a)}=<b>${esc(d)}</b>`).join(' ')}</div>
        <div class="seq-now">${activeTxt(b) || '&nbsp;'}</div>
        <div class="row" style="flex-wrap:wrap;gap:4px">${cmdButtons(b, op)}</div></div>`).join('') || '<p class="muted">Nessun batch attivo. Avvia una ricetta con “▶ Nuovo batch”.</p>'}</div>
      <div id="bDet"></div>`;
    body.querySelector('#bNew').onclick = newBatch;
    body.querySelectorAll('[data-k]').forEach((c) => { c.onclick = () => { sel = c.dataset.k; drawBatch(); }; wireCmds(c, c.dataset.k); });
    const b = batches.find((x) => x.key === sel);
    if (!b) return;
    const recipe = st.recipes.find((r) => r.name === b.name);
    const phases = st.instances.filter((i) => i.owner === b.key);
    const det = body.querySelector('#bDet');
    det.innerHTML = `<div class="grid2" style="grid-template-columns:minmax(380px,1fr) minmax(380px,1fr)">
      <div class="card"><h3>${esc(b.key)} – procedura ${esc(b.name)} v${b.version}${recipe && recipe.version !== b.version ? ` <span class="chip warn">ricetta ora v${recipe.version}</span>` : ''}</h3>${sfcFlowView(b.steps, live ? liveRun(b, live, phases) : runInfo(b))}</div>
      <div class="card"><h3>Fasi del batch</h3>
        <table class="ph-list"><thead><tr><th>Fase</th><th>Stato</th><th>Passo</th><th>Parametri</th><th></th></tr></thead><tbody>${phases.map((p) => `<tr data-ph="${esc(p.key)}"><td><a href="#/process?tab=phases&sel=${encodeURIComponent(p.key)}"><b>${esc(p.key)}</b></a></td><td>${chip(p.state)}</td><td>${activeTxt(p)}</td>
          <td class="mono" style="font-size:11.5px">${Object.entries(p.params).map(([k, v]) => `${esc(k)}=${esc(v)}`).join('<br>')}</td>
          <td class="nowrap">${p.state === 'error' ? `<button class="small primary" data-cmd="restart" ${can('batch.operate') ? '' : 'disabled'}>▶ Riprova</button>` : ''}</td></tr>`).join('') || '<tr><td colspan="5" class="muted">nessuna fase avviata</td></tr>'}</tbody></table>
        <h3 style="margin-top:14px">Esecuzione</h3>
        <p>Avvio ${fmtTs(b.started)} da <b>${esc(b.user)}</b> · parametri ${Object.entries(b.params).map(([k, v]) => `<code>${esc(k)}=${esc(v)}</code>`).join(' ')}</p>
        <table><thead><tr><th>Passo</th><th>Ingresso</th><th>Durata</th><th>Uscita</th></tr></thead><tbody>${b.history.map((h) => `<tr><td>${esc(h.path)} ${esc(h.step)}</td><td>${new Date(h.enter).toLocaleTimeString()}</td><td>${h.exit ? dur(h.exit - h.enter) : '…'}</td><td class="muted">${esc(h.reason)}</td></tr>`).join('')}</tbody></table></div></div>
      ${live ? liveHtml(b, { live, sequences: st.sequences, children: phases }) : ''}`;
    det.querySelectorAll('[data-ph]').forEach((r) => wireCmds(r, r.dataset.ph));
  }

  async function newBatch() {
    const recs = st.recipes;
    let r = recs[0];
    const back = modal('Nuovo batch', '<div id="nb"></div>', [['Annulla', '', null], ['▶ Avvia batch', 'primary', async (b) => {
      const devices = Object.fromEntries([...b.querySelectorAll('[data-dev]')].map((x) => [x.dataset.dev, x.value]));
      const params = Object.fromEntries([...b.querySelectorAll('[data-par]')].map((x) => [x.dataset.par, x.value]));
      try {
        const i = await api.post('/api/batches', { recipe: r.name, devices, params, label: b.querySelector('#nbLabel').value.trim() });
        toast(`Batch ${i.key} avviato: ${r.name} su ${Object.values(i.devices).join(', ')}`, 'ok');
        sel = i.key; await reload(); draw();
      } catch (e) { toast(e.message, 'error'); return false; }
    }]], { wide: true });
    const fill = () => {
      back.querySelector('#nb').innerHTML = `<div class="field"><label>Ricetta</label><select id="nbR">${recs.map((x) => `<option ${x === r ? 'selected' : ''}>${esc(x.name)}</option>`).join('')}</select></div>
        <p class="hint">${esc(r.description || '')} · versione ${r.version}</p>
        <div class="field"><label>Lotto / descrizione</label><input id="nbLabel" placeholder="es. L2611-03"></div>
        <h4>Dispositivi (allocati in esclusiva per tutto il batch)</h4>
        ${r.devices.map((d) => `<div class="field"><label>${esc(d.alias)} <span class="muted">(${esc(d.class)})</span></label><select data-dev="${esc(d.alias)}">${st.devices.filter((x) => x.class === d.class).map((x) => {
          const busy = x.batch ? `allocato a ${x.batch}` : x.phases.length ? `fase manuale ${x.phases.join(', ')}` : '';
          return `<option value="${esc(x.name)}" ${busy ? 'disabled' : ''} ${!busy && (x.name === d.default) ? 'selected' : ''}>${esc(x.name)}${busy ? ` – ${busy}` : ''}</option>`;
        }).join('')}</select></div>`).join('') || '<p class="muted">nessun dispositivo</p>'}
        <h4>Parametri</h4>
        ${r.parameters.map((p) => `<div class="field"><label>${esc(p.name)} ${p.unit ? `<span class="muted">${esc(p.unit)}</span>` : ''}</label><div class="row"><input data-par="${esc(p.name)}" value="${esc(p.default ?? '')}" style="width:140px">
          <span class="muted">${p.min !== undefined || p.max !== undefined ? `${p.min ?? '−∞'} … ${p.max ?? '+∞'}` : ''} ${esc(p.description || '')}</span></div></div>`).join('') || '<p class="muted">nessun parametro</p>'}`;
      back.querySelector('#nbR').onchange = (e) => { r = recs.find((x) => x.name === e.target.value); fill(); };
    };
    fill();
  }

  // ------------------------------------------------------------ fasi
  function drawPhases() {
    const op = can('seq.operate');
    const insts = st.instances.filter((i) => i.kind === 'phase');
    body.innerHTML = `<p class="hint">Fasi (sequenze SFC) in manuale. Una fase avviata da un batch si comanda dal batch. I dispositivi allocati a un batch non sono disponibili.</p>
      <div class="seq-grid">${st.sequences.map((s) => {
        const mine = insts.filter((i) => i.name === s.name);
        return `<div class="card seq-card"><div class="row"><b class="grow">${esc(s.name)}</b><span class="muted">${esc(s.device_class || '')}</span></div>
          <div class="muted" style="font-size:12.5px">${esc(s.description || '')}${s.parameters?.length ? ` · parametri: ${s.parameters.map((p) => esc(p.name)).join(', ')}` : ''}</div>
          ${mine.map((i) => `<div class="ph-inst" data-k="${esc(i.key)}"><div class="row"><b>${esc(i.device || '—')}</b>${chip(i.state)}${i.owner ? `<span class="chip">batch ${esc(i.owner)}</span>` : ''}<span class="grow"></span><button class="small" data-show>◫</button></div>
            <div class="seq-now">${activeTxt(i)}</div><div class="row" style="flex-wrap:wrap;gap:4px">${i.owner ? '' : cmdButtons(i, op)}</div></div>`).join('')}
          <button class="small primary" data-start="${esc(s.name)}" ${op ? '' : 'disabled'} style="margin-top:6px">▶ Avvia…</button></div>`;
      }).join('') || '<p class="muted">Nessuna sequenza (Designer → ⚗ Processo).</p>'}</div><div id="phDet"></div>`;
    body.querySelectorAll('[data-start]').forEach((b) => (b.onclick = () => startPhase(st.sequences.find((s) => s.name === b.dataset.start))));
    body.querySelectorAll('.ph-inst').forEach((r) => { wireCmds(r, r.dataset.k); r.querySelector('[data-show]').onclick = () => { sel = r.dataset.k; drawPhases(); }; });
    const i = insts.find((x) => x.key === sel) || insts.find((x) => ['running', 'held', 'error'].includes(x.state));
    if (i) {
      body.querySelectorAll('.ph-inst').forEach((r) => r.classList.toggle('sel', r.dataset.k === i.key));
      body.querySelector('#phDet').innerHTML = `<div class="card"><h3>${esc(i.key)} ${chip(i.state)}${i.owner ? ` <span class="chip">batch ${esc(i.owner)}</span>` : ''}</h3>${sfcFlowView(i.steps, live ? liveRun(i, live) : runInfo(i))}</div>
        ${live ? liveHtml(i, { live, sequences: st.sequences }) : ''}`;
    }
  }

  function startPhase(s) {
    const devs = st.devices.filter((d) => !s.device_class || d.class === s.device_class);
    modal(`Avvio fase ${s.name}`, `${s.device_class ? `<div class="field"><label>Dispositivo</label><select id="spDev">${devs.map((d) => `<option value="${esc(d.name)}" ${d.batch ? 'disabled' : ''} ${d.name === s.unit ? 'selected' : ''}>${esc(d.name)}${d.batch ? ` – allocato a ${esc(d.batch)}` : ''}</option>`).join('')}</select></div>` : ''}
      ${(s.parameters || []).map((p) => `<div class="field"><label>${esc(p.name)} ${p.unit ? `<span class="muted">${esc(p.unit)}</span>` : ''}</label><div class="row"><input data-par="${esc(p.name)}" value="${esc(p.default ?? '')}" style="width:140px"><span class="muted">${p.min !== undefined || p.max !== undefined ? `${p.min ?? '−∞'} … ${p.max ?? '+∞'}` : ''}</span></div></div>`).join('')}`,
    [['Annulla', '', null], ['▶ Avvia', 'primary', async (b) => {
      const params = Object.fromEntries([...b.querySelectorAll('[data-par]')].map((x) => [x.dataset.par, x.value]));
      try { const i = await api.post(`/api/phase/${encodeURIComponent(s.name)}/start`, { device: b.querySelector('#spDev')?.value || '', params }); sel = i.key; await reload(); draw(); } catch (e) { toast(e.message, 'error'); return false; }
    }]]);
  }

  // ------------------------------------------------------------ ricette
  let editing = null, rsel = params.recipe || '', rview = 'diag';
  function drawRecipes() {
    const edit = can('recipe.edit');
    if (!editing && !st.recipes.some((r) => r.name === rsel)) rsel = st.recipes[0]?.name || '';
    const r = editing || st.recipes.find((x) => x.name === rsel);
    body.innerHTML = `<div class="grid2" style="grid-template-columns:260px 1fr">
      <div class="card"><div class="row"><h3 class="grow" style="margin:0">Ricette</h3><button class="small primary" id="rNew" ${edit ? '' : 'disabled'}>+ Nuova</button></div>
        <ul class="list">${st.recipes.map((x) => `<li data-r="${esc(x.name)}" class="${x.name === rsel && !editing ? 'on' : ''}"><span class="vt"><b>${esc(x.name)}</b> <span class="muted">v${x.version}</span><br><span class="muted" style="font-size:12px">${esc(x.description || '')}</span></span></li>`).join('') || '<li class="muted">nessuna</li>'}</ul></div>
      <div class="card" id="rEd">${r ? '' : '<p class="muted">Seleziona o crea una ricetta.</p>'}</div></div>`;
    body.querySelector('#rNew').onclick = () => {
      editing = { name: '', description: '', devices: [], parameters: [], steps: [{ name: 'Passo 1' }], _new: true };
      const cls = [...new Set(st.devices.map((d) => d.class))][0];
      if (cls) editing.devices.push({ alias: cls.toLowerCase(), class: cls, default: st.devices.find((d) => d.class === cls)?.name || '' });
      rview = 'edit'; drawRecipes();
    };
    body.querySelectorAll('[data-r]').forEach((li) => (li.onclick = () => { if (editing && !confirm('Abbandonare le modifiche?')) return; editing = null; rsel = li.dataset.r; rview = 'diag'; drawRecipes(); }));
    if (!r) return;
    const ed = body.querySelector('#rEd');
    const isEdit = !!editing;
    const classes = [...new Set(st.devices.map((d) => d.class))];
    ed.innerHTML = `<div class="row" style="flex-wrap:wrap;gap:6px"><h3 class="grow" style="margin:0">${isEdit && r._new ? 'Nuova ricetta' : esc(r.name)} ${r.version ? `<span class="chip">v${r.version}</span>` : ''}${r.modified ? `<span class="muted" style="font-size:12px"> · ${fmtTs(r.modified)} ${esc(r.modified_by || '')}</span>` : ''}</h3>
        ${isEdit ? `<button class="primary" id="rSave">💾 Salva</button><button id="rCancel">Annulla</button>` : `<button id="rEdit" ${edit ? '' : 'disabled'}>✎ Modifica</button><button id="rCopy" ${edit ? '' : 'disabled'}>⧉ Duplica</button><button class="danger" id="rDel" ${edit ? '' : 'disabled'}>Elimina</button>
        <button class="primary" id="rRun" ${can('batch.operate') ? '' : 'disabled'}>▶ Avvia batch</button>`}</div>
      ${isEdit ? `<div class="formgrid" style="margin-top:8px">${r._new ? '<label>Nome</label><input id="rName" placeholder="es. Crema">' : ''}<label>Descrizione</label><input id="rDesc" value="${esc(r.description || '')}"></div>
        <h4>Dispositivi richiesti ${helpBtn('runtime#ricette')}</h4><div class="hint">Alias usato nelle fasi come <code>{alias}</code>; all'avvio del batch l'operatore sceglie il dispositivo della classe, che viene allocato in esclusiva.</div>
        <table class="edit"><thead><tr><th>Alias</th><th>Classe</th><th>Predefinito</th><th></th></tr></thead><tbody>${r.devices.map((d, k) => `<tr data-dv="${k}"><td><input data-f="alias" value="${esc(d.alias)}" style="width:120px"></td>
          <td><select data-f="class">${classes.map((c) => `<option ${c === d.class ? 'selected' : ''}>${esc(c)}</option>`).join('')}</select></td>
          <td><select data-f="default"><option value=""></option>${st.devices.filter((x) => x.class === d.class).map((x) => `<option ${x.name === d.default ? 'selected' : ''}>${esc(x.name)}</option>`).join('')}</select></td><td><button class="small danger" data-dvdel>✕</button></td></tr>`).join('')}</tbody></table>
        <button class="small" id="dvAdd">+ Dispositivo</button>
        <h4>Parametri della ricetta</h4><div class="hint">Passati alle fasi come <code>{nome}</code>; il default è il valore di formula, modificabile all'avvio entro i limiti.</div>
        <table class="edit"><thead><tr><th>Nome</th><th>Unità</th><th>Min</th><th>Max</th><th>Default</th><th>Descrizione</th><th></th></tr></thead><tbody>${r.parameters.map((p, k) => `<tr data-pr="${k}">${['name', 'unit', 'min', 'max', 'default', 'description'].map((f) => `<td><input data-f="${f}" value="${esc(p[f] ?? '')}" style="width:${f === 'description' ? 180 : f === 'name' ? 110 : 70}px"></td>`).join('')}<td><button class="small danger" data-prdel>✕</button></td></tr>`).join('')}</tbody></table>
        <button class="small" id="prAdd">+ Parametro</button>
        <h4>Procedura</h4><div id="rSfc"></div>
        <h4>Azioni di stato</h4>${stateActionsHtml(r)}`
      : `<p class="muted">${esc(r.description || '')}</p>
        <p>Dispositivi: ${r.devices.map((d) => `<span class="chip">{${esc(d.alias)}} ${esc(d.class)}${d.default ? ` → ${esc(d.default)}` : ''}</span>`).join('') || '—'}</p>
        <p>Parametri: ${r.parameters.map((p) => `<span class="chip">${esc(p.name)} = ${esc(p.default ?? '—')} ${esc(p.unit || '')}</span>`).join('') || '—'}</p>
        ${sfcFlowView(r.steps, { subst: (t) => t })}`}`;
    if (!isEdit) {
      ed.querySelector('#rEdit').onclick = () => { editing = structuredClone(r); rview = 'edit'; drawRecipes(); };
      ed.querySelector('#rCopy').onclick = () => { editing = { ...structuredClone(r), _new: true, version: 0 }; drawRecipes(); ed.querySelector('#rName') && (ed.querySelector('#rName').value = `${r.name}_copia`); };
      ed.querySelector('#rDel').onclick = async () => { if (!confirm(`Eliminare la ricetta ${r.name}?`)) return; try { await api.del(`/api/recipes/${encodeURIComponent(r.name)}`); await reload(); rsel = ''; drawRecipes(); } catch (e) { toast(e.message, 'error'); } };
      ed.querySelector('#rRun').onclick = () => { location.hash = '#/process?tab=batch'; setTimeout(() => document.querySelector('#bNew')?.click(), 400); };
      return;
    }
    const redraw = () => drawRecipes();
    ed.querySelector('#rCancel').onclick = () => { editing = null; redraw(); };
    ed.querySelector('#rDesc').onchange = (e) => (r.description = e.target.value);
    ed.querySelectorAll('[data-dv]').forEach((tr) => {
      const d = r.devices[+tr.dataset.dv];
      tr.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => { d[el.dataset.f] = el.value.trim(); if (el.dataset.f === 'class') { d.default = ''; redraw(); } }));
      tr.querySelector('[data-dvdel]').onclick = () => { r.devices.splice(+tr.dataset.dv, 1); redraw(); };
    });
    ed.querySelector('#dvAdd').onclick = () => { r.devices.push({ alias: `disp${r.devices.length + 1}`, class: classes[0] || '', default: '' }); redraw(); };
    ed.querySelectorAll('[data-pr]').forEach((tr) => {
      const p = r.parameters[+tr.dataset.pr];
      tr.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
        const f = el.dataset.f, v = el.value.trim();
        if (['min', 'max'].includes(f)) { if (v === '') delete p[f]; else p[f] = +v; }
        else if (f === 'default') { if (v === '') delete p[f]; else p[f] = isNaN(+v) ? v : +v; }
        else p[f] = v;
      }));
      tr.querySelector('[data-prdel]').onclick = () => { r.parameters.splice(+tr.dataset.pr, 1); redraw(); };
    });
    ed.querySelector('#prAdd').onclick = () => { r.parameters.push({ name: `P${r.parameters.length + 1}` }); redraw(); };
    sfcFlow(ed.querySelector('#rSfc'), r.steps, { mode: 'recipe', ctx: { sequences: st.sequences, functions: [], aliases: () => r.devices.map((d) => d.alias), params: () => r.parameters.map((p) => p.name) } });
    wireStateActions(ed, r, (re) => re && redraw());
    ed.querySelector('#rSave').onclick = async () => {
      const name = r._new ? ed.querySelector('#rName').value.trim() : r.name;
      if (!name) return toast('Nome della ricetta mancante', 'error');
      const body2 = structuredClone(r);
      delete body2._new;
      try {
        const saved = await api.put(`/api/recipes/${encodeURIComponent(name)}`, { ...body2, name });
        toast(`Ricetta ${name} salvata (v${saved.version})`, 'ok');
        editing = null; rsel = name; await reload(); redraw();
      } catch (e) { toast(e.message, 'error'); }
    };
  }

  // ------------------------------------------------------------ dispositivi
  function drawDevices() {
    const byClass = {};
    for (const d of st.devices) (byClass[d.class || '—'] ||= []).push(d);
    body.innerHTML = Object.entries(byClass).map(([c, ds]) => `<h3>${esc(c)}</h3><div class="dev-grid">${ds.map((d) => `<div class="card dev-card ${d.batch ? 'alloc' : ''}">
      <div class="row"><b class="grow">${esc(d.name)}</b>${d.batch ? '<span class="chip warn">allocato</span>' : '<span class="chip ok">libero</span>'}</div>
      <div class="muted" style="font-size:12.5px">${esc(d.description || '')}</div>
      ${d.batch ? `<div style="margin-top:6px">batch <a href="#/process?tab=batch&sel=${encodeURIComponent(d.batch)}"><b>${esc(d.batch)}</b></a> · ${esc(d.recipe || '')}</div>` : ''}
      ${d.phases.length ? `<div class="muted" style="margin-top:4px">fasi: ${d.phases.map(esc).join(', ')}</div>` : ''}</div>`).join('')}</div>`).join('') || '<p class="muted">Nessun dispositivo (Designer → ⚗ Processo → Dispositivi).</p>';
  }

  // ------------------------------------------------------------ storico
  async function drawHistory() {
    const rows = await api.get('/api/proc/history?limit=100');
    body.innerHTML = `<div class="card"><h3>Batch conclusi</h3><table><thead><tr><th>Batch</th><th>Ricetta</th><th>Lotto</th><th>Dispositivi</th><th>Esito</th><th>Avvio</th><th>Durata</th><th>Utente</th><th>Messaggio</th></tr></thead><tbody>
      ${rows.map((b, k) => `<tr data-h="${k}" style="cursor:pointer"><td><b>${esc(b.key)}</b></td><td>${esc(b.name)} v${b.version}</td><td>${esc(b.label || '')}</td><td>${Object.values(b.devices || {}).map(esc).join(', ')}</td>
        <td>${chip(b.state)}</td><td>${fmtTs(b.started)}</td><td>${b.finished ? dur(b.finished - b.started) : ''}</td><td>${esc(b.user)}</td><td class="muted">${esc(b.message || '')}</td></tr>`).join('') || '<tr><td colspan="9" class="muted">nessun batch concluso</td></tr>'}</tbody></table></div><div id="hDet"></div>`;
    body.querySelectorAll('[data-h]').forEach((tr) => (tr.onclick = () => {
      const b = rows[+tr.dataset.h];
      body.querySelector('#hDet').innerHTML = `<div class="card"><h3>Report ${esc(b.key)} – ${esc(b.name)} v${b.version}</h3>
        <p>Parametri: ${Object.entries(b.params || {}).map(([k, v]) => `<code>${esc(k)}=${esc(v)}</code>`).join(' ')} · dispositivi ${Object.entries(b.devices || {}).map(([a, d]) => `${esc(a)}=${esc(d)}`).join(', ')}</p>
        <div class="grid2"><div><h4>Passi della procedura</h4><table><tbody>${(b.history || []).map((h) => `<tr><td>${esc(h.path)} ${esc(h.step)}</td><td>${new Date(h.enter).toLocaleTimeString()}</td><td>${h.exit ? dur(h.exit - h.enter) : ''}</td><td class="muted">${esc(h.reason)}</td></tr>`).join('')}</tbody></table></div>
        <div><h4>Fasi</h4><table><tbody>${(b.phases || []).map((p) => `<tr><td><a href="#/process?tab=phases&sel=${encodeURIComponent(p.key)}"><b>${esc(p.key)}</b></a></td><td>${chip(p.state)}</td><td class="mono" style="font-size:11.5px">${Object.entries(p.params || {}).map(([k, v]) => `${esc(k)}=${esc(v)}`).join(' ')}</td><td>${p.finished ? dur(p.finished - p.started) : ''}</td></tr>`).join('')}</tbody></table></div></div></div>`;
    }));
  }

  function draw() {
    ({ batch: drawBatch, phases: drawPhases, recipes: drawRecipes, devices: drawDevices, history: drawHistory })[tab]();
  }
  draw();
  // aggiornamento periodico (non durante la modifica di una ricetta)
  setTimer(async () => {
    if (tab === 'history' || (tab === 'recipes' && editing) || document.querySelector('.modal-back')) return;
    await reload();
    if (tab !== 'recipes') draw();
  }, 1000);
}

// ================================================================ audit
const CATS = { comandi: '🖱', configurazione: '⚙', sicurezza: '🔐', allarmi: '🔔', ricette: '🧪', sequenze: '🔁', wcs: '🏬', altro: '·' };
const ACTIONS = {
  write: 'Scrittura tag', ack: 'Riconoscimento allarmi', login: 'Accesso', logout: 'Uscita', login_failed: 'Accesso fallito', change_password: 'Cambio password',
  user_save: 'Utente salvato', user_delete: 'Utente eliminato', save_project: 'Progetto salvato', save_view: 'Vista salvata', delete_view: 'Vista eliminata',
  save_rules: 'Regole salvate', save_script: 'Script salvato', delete_script: 'Script eliminato', script_call: 'Script eseguito', recipe_save: 'Ricetta salvata',
  recipe_create: 'Ricetta creata', batch_start: 'Avvio batch', batch_hold: 'Hold batch', batch_restart: 'Restart batch', batch_stop: 'Stop batch', batch_abort: 'Abort batch', batch_reset: 'Reset batch', batch_confirm: 'Conferma (batch)', seq_hold: 'Hold fase', seq_restart: 'Restart fase', seq_abort: 'Abort fase', recipe_delete: 'Ricetta eliminata', seq_start: 'Avvio sequenza', seq_stop: 'Arresto sequenza', seq_pause: 'Pausa sequenza',
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
