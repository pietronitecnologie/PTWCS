// Workspace "Processo": tipi di ricetta (parametri → tag) e sequenze a passi.
import { esc } from './core.js';
import { P, markDirty, areaNames } from './project.js';
import { tableEditor } from './forms.js';
import { helpBtn } from './markdown.js';

const OPS = ['==', '!=', '>', '>=', '<', '<='];
const KINDS = [['write', 'Scrivi tag'], ['recipe', 'Carica ricetta'], ['script', 'Script Python'], ['log', 'Messaggio nel log']];

export function mount(root) {
  let tab = 'seq', selR = 0, selS = 0, open = new Set([0]);
  root.innerHTML = `<div class="wsl"><div class="side" id="prSide"></div><div class="main" id="prMain"></div></div>`;
  const side = root.querySelector('#prSide'), main = root.querySelector('#prMain');
  const data = () => { P.data.recipe_types ||= []; P.data.sequences ||= []; return P.data; };

  function drawSide() {
    const d = data();
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">
        <button data-st="seq" class="${tab === 'seq' ? 'on' : ''}" style="text-align:left">🔁 Sequenze (${d.sequences.length})</button>
        <button data-st="rcp" class="${tab === 'rcp' ? 'on' : ''}" style="text-align:left">🧪 Tipi di ricetta (${d.recipe_types.length})</button></div>
      <ul class="list">${(tab === 'seq' ? d.sequences : d.recipe_types).map((x, i) => `<li data-i="${i}" class="${i === (tab === 'seq' ? selS : selR) ? 'on' : ''}"><span class="vt">${esc(x.name)}</span></li>`).join('')}</ul>
      <button class="small" id="prNew" style="margin-top:6px">+ ${tab === 'seq' ? 'Nuova sequenza' : 'Nuovo tipo di ricetta'}</button>
      <p class="hint">Si applicano con <b>Salva progetto</b>. Le ricette (valori) si gestiscono dal runtime: <b>Processo → Ricette</b>.</p>`;
    side.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { tab = b.dataset.st; drawSide(); draw(); }));
    side.querySelectorAll('[data-i]').forEach((li) => (li.onclick = () => { if (tab === 'seq') { selS = +li.dataset.i; open = new Set([0]); } else selR = +li.dataset.i; drawSide(); draw(); }));
    side.querySelector('#prNew').onclick = () => {
      const n = prompt(tab === 'seq' ? 'Nome della sequenza:' : 'Nome del tipo di ricetta:');
      if (!n) return;
      if (tab === 'seq') { d.sequences.push({ name: n.trim(), steps: [{ name: 'Passo 1', transitions: [{ next: 'END' }] }] }); selS = d.sequences.length - 1; }
      else { d.recipe_types.push({ name: n.trim(), parameters: [] }); selR = d.recipe_types.length - 1; }
      markDirty(); drawSide(); draw();
    };
  }

  function draw() { (tab === 'seq' ? drawSeq : drawRcp)(); }

  // ------------------------------------------------------------ tipi di ricetta
  function drawRcp() {
    const list = data().recipe_types;
    const r = list[selR];
    if (!r) { main.innerHTML = '<p class="muted">Nessun tipo di ricetta. Crea il primo con “+ Nuovo tipo di ricetta”.</p>'; return; }
    main.innerHTML = `<div class="sec"><h3>Tipo di ricetta ${helpBtn('designer-processo#tipi-di-ricetta')}<span class="grow"></span><button class="small danger" id="rDel">Elimina</button></h3>
      <div class="formgrid">
        <label>Nome</label><input data-f="name" value="${esc(r.name)}">
        <label>Descrizione</label><input data-f="description" value="${esc(r.description || '')}">
        <label>Area</label><select data-f="area"><option value=""></option>${areaNames().map((a) => `<option ${a === r.area ? 'selected' : ''}>${esc(a)}</option>`).join('')}</select>
        <label>Unità</label><div class="row"><input data-f="units" value="${esc((r.units || []).join(', '))}" placeholder="R1, R2 (vuoto = unità singola)"><span class="hint" style="margin:0">nei tag usa <code>{unit}</code></span></div>
      </div></div>
      <div class="sec"><h3>Parametri ${helpBtn('designer-processo#parametri')}</h3><p class="hint">Ogni parametro viene scritto sul suo tag al caricamento della ricetta. Limiti min/max verificati al salvataggio e al caricamento.</p><div id="rPar"></div></div>`;
    main.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
      const k = el.dataset.f;
      if (k === 'units') r.units = el.value.split(',').map((x) => x.trim()).filter(Boolean);
      else if (el.value === '' && k !== 'name') delete r[k]; else r[k] = el.value;
      markDirty(); if (k === 'name') drawSide();
    }));
    main.querySelector('#rDel').onclick = () => { if (confirm(`Eliminare il tipo di ricetta ${r.name}? Le ricette salvate restano nel database.`)) { list.splice(selR, 1); selR = 0; markDirty(); drawSide(); draw(); } };
    tableEditor(main.querySelector('#rPar'), r.parameters, [
      { key: 'name', label: 'Parametro', width: 120, required: true },
      { key: 'tag', label: 'Tag', width: 170, list: 'tagNames', placeholder: '{unit}.SP_Temp' },
      { key: 'unit', label: 'Unità', width: 50 },
      { key: 'min', label: 'Min', type: 'number', width: 70 }, { key: 'max', label: 'Max', type: 'number', width: 70 },
      { key: 'default', label: 'Default', width: 80 },
      { key: 'description', label: 'Descrizione', width: 200 },
    ], { onChange: () => {
      for (const p of r.parameters) for (const k of ['min', 'max']) if (p[k] === '' || p[k] === null || Number.isNaN(p[k])) delete p[k];
      for (const p of r.parameters) if (typeof p.default === 'string' && p.default.trim() !== '' && !isNaN(+p.default)) p.default = +p.default;
      markDirty();
    }, newRow: () => ({ name: `P${r.parameters.length + 1}`, tag: r.units?.length ? '{unit}.' : '' }) });
  }

  // ------------------------------------------------------------ sequenze
  function drawSeq() {
    const list = data().sequences;
    const s = list[selS];
    if (!s) { main.innerHTML = '<p class="muted">Nessuna sequenza. Crea la prima con “+ Nuova sequenza”.</p>'; return; }
    s.steps ||= [];
    const stepNames = s.steps.map((x) => x.name);
    const nextOpts = (v, withEmpty = true) => `${withEmpty ? `<option value="" ${!v ? 'selected' : ''}>(passo seguente)</option>` : ''}${stepNames.map((n) => `<option ${n === v ? 'selected' : ''}>${esc(n)}</option>`).join('')}<option value="END" ${v === 'END' ? 'selected' : ''}>FINE</option>`;
    const rtypes = data().recipe_types.map((r) => r.name);
    const actHtml = (a, si, ai, list = 'actions') => `<div class="pr-row" data-s="${si}" data-a="${ai}" data-l="${list}">
        <select data-k="kind">${KINDS.map(([k, l]) => `<option value="${k}" ${k === a.kind ? 'selected' : ''}>${l}</option>`).join('')}</select>
        ${a.kind === 'write' ? `<input data-k="tag" list="tagNames" value="${esc(a.tag || '')}" placeholder="{unit}.Tag"><span>←</span><input data-k="value" value="${esc(a.value === undefined ? 'true' : String(a.value))}" style="width:90px">`
        : a.kind === 'recipe' ? `<select data-k="recipe_type">${rtypes.map((r) => `<option ${r === a.recipe_type ? 'selected' : ''}>${esc(r)}</option>`).join('')}</select><input data-k="recipe" value="${esc(a.recipe || '')}" placeholder="nome o {recipe}">`
        : a.kind === 'script' ? `<select data-k="fn">${(P.functions || []).map((f) => `<option ${f === a.fn ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select>`
        : `<input data-k="text" value="${esc(a.text || '')}" placeholder="testo, es. batch {unit} avviato" style="flex:1">`}
        <button class="small danger" data-del>✕</button></div>`;
    main.innerHTML = `<div class="sec"><h3>Sequenza ${helpBtn('designer-processo#sequenze')}<span class="grow"></span><button class="small" id="sDup">Duplica</button><button class="small danger" id="sDel">Elimina</button></h3>
      <div class="formgrid">
        <label>Nome</label><input data-f="name" value="${esc(s.name)}">
        <label>Descrizione</label><input data-f="description" value="${esc(s.description || '')}">
        <label>Area</label><select data-f="area"><option value=""></option>${areaNames().map((a) => `<option ${a === s.area ? 'selected' : ''}>${esc(a)}</option>`).join('')}</select>
        <label>Unità</label><div class="row"><input data-f="unit" value="${esc(s.unit || '')}" placeholder="es. R1" style="width:110px"><span class="hint" style="margin:0">sostituisce <code>{unit}</code> in tag, valori e ricette</span></div>
        <label>Avvio da PLC</label><div class="row"><input data-f="start_tag" list="tagNames" value="${esc(s.start_tag || '')}" placeholder="tag bool (fronte di salita)"></div>
        <label>Parametri di avvio</label><div class="row"><input data-f="params" value="${esc(Object.entries(s.params || {}).map(([k, v]) => `${k}=${v}`).join(', '))}" placeholder="recipe=Standard"><span class="hint" style="margin:0">usabili come <code>{nome}</code></span></div>
      </div></div>
      <div class="sec"><h3>Passi ${helpBtn('designer-processo#passi-e-transizioni')}<span class="grow"></span><button class="small" id="sAll">Espandi tutti</button></h3>
        ${s.steps.map((st, i) => `<div class="pr-step ${open.has(i) ? 'open' : ''}" data-si="${i}">
          <div class="pr-head"><span class="sfc-n">${i + 1}</span><input data-sf="name" value="${esc(st.name)}" style="width:170px;font-weight:700">
            <span class="muted grow" style="font-size:12px">${(st.actions || []).length} azioni · ${(st.transitions || []).length} transizioni${st.timeout_s ? ` · timeout ${st.timeout_s} s` : ''}</span>
            <button class="small" data-mv="-1" title="Su">↑</button><button class="small" data-mv="1" title="Giù">↓</button><button class="small" data-ins title="Inserisci un passo dopo">+</button>
            <button class="small danger" data-sdel title="Elimina passo">✕</button><button class="small" data-tg>${open.has(i) ? '▾' : '▸'}</button></div>
          ${open.has(i) ? `<div class="pr-body">
            <div class="row" style="flex-wrap:wrap;gap:10px"><label class="row">Descrizione <input data-sf="description" value="${esc(st.description || '')}" style="width:260px"></label>
              <label class="row" title="Tempo minimo prima di valutare le transizioni">T. minimo (s) <input type="number" data-sf="min_time_s" value="${st.min_time_s ?? ''}" style="width:70px"></label>
              <label class="row" title="0 = nessun timeout">Timeout (s) <input type="number" data-sf="timeout_s" value="${st.timeout_s ?? ''}" style="width:70px"></label>
              <label class="row">in timeout → <select data-sf="on_timeout"><option value="" ${!st.on_timeout ? 'selected' : ''}>errore + allarme</option>${stepNames.map((n) => `<option ${n === st.on_timeout ? 'selected' : ''}>${esc(n)}</option>`).join('')}</select></label></div>
            <h5>⚡ Azioni all'ingresso</h5>${(st.actions || []).map((a, ai) => actHtml(a, i, ai)).join('')}<button class="small" data-aadd>+ Azione</button>
            <h5>➜ Transizioni (la prima vera vince)</h5>
            ${(st.transitions || []).map((t, ti) => `<div class="pr-tr" data-ti="${ti}">
              <div class="row" style="gap:6px;flex-wrap:wrap"><b>Se</b><select data-tf="mode"><option value="all" ${t.mode !== 'any' ? 'selected' : ''}>tutte</option><option value="any" ${t.mode === 'any' ? 'selected' : ''}>almeno una</option></select>
                <span>vai a</span><select data-tf="next">${nextOpts(t.next)}</select><input data-tf="label" value="${esc(t.label || '')}" placeholder="etichetta" style="width:150px">
                <span class="grow"></span><button class="small" data-cadd>+ Condizione</button><button class="small danger" data-tdel>✕</button></div>
              ${(t.when || []).map((c, ci) => `<div class="pr-row" data-ci="${ci}"><input data-cf="tag" list="seqTags" value="${esc(c.tag)}" placeholder="tag, @elapsed, @confirm">
                <select data-cf="op">${OPS.map((o) => `<option ${o === c.op ? 'selected' : ''}>${o}</option>`).join('')}</select>
                <input data-cf="value" value="${esc(String(c.value ?? ''))}" placeholder="valore o tag:Nome" style="width:170px"><button class="small danger" data-cdel>✕</button></div>`).join('') || '<div class="muted" style="font-size:12px;margin:4px 0">nessuna condizione: transizione immediata</div>'}
            </div>`).join('')}<button class="small" data-tadd>+ Transizione</button>
          </div>` : ''}</div>`).join('')}
        <button class="small" id="sAdd" style="margin-top:6px">+ Passo in fondo</button></div>
      <div class="sec"><h3>Azioni di arresto ${helpBtn('designer-processo#azioni-di-arresto')}</h3><p class="hint">Eseguite al comando di arresto: porta l'impianto in stato sicuro.</p>
        <div data-stop>${(s.stop_actions || []).map((a, ai) => actHtml(a, -1, ai, 'stop')).join('')}</div><button class="small" id="stAdd">+ Azione</button></div>
      <datalist id="seqTags"><option value="@elapsed">secondi nel passo</option><option value="@confirm">conferma operatore</option></datalist>`;
    const redraw = () => { markDirty(); drawSeq(); };
    main.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
      const k = el.dataset.f;
      if (k === 'params') {
        const o = Object.fromEntries(el.value.split(',').map((x) => x.split('=').map((y) => y.trim())).filter((x) => x[0]).map(([a, b]) => [a, b ?? '']));
        if (Object.keys(o).length) s.params = o; else delete s.params;
      } else if (el.value === '' && k !== 'name') delete s[k]; else s[k] = el.value.trim();
      markDirty(); if (k === 'name') drawSide();
    }));
    main.querySelector('#sDel').onclick = () => { if (confirm(`Eliminare la sequenza ${s.name}?`)) { list.splice(selS, 1); selS = 0; markDirty(); drawSide(); draw(); } };
    main.querySelector('#sDup').onclick = () => { const c = structuredClone(s); c.name += '_copia'; delete c.start_tag; list.push(c); selS = list.length - 1; markDirty(); drawSide(); draw(); };
    main.querySelector('#sAll').onclick = () => { open = open.size === s.steps.length ? new Set() : new Set(s.steps.map((_, i) => i)); drawSeq(); };
    main.querySelector('#sAdd').onclick = () => { s.steps.push({ name: `Passo ${s.steps.length + 1}`, transitions: [{ next: '' }] }); open.add(s.steps.length - 1); redraw(); };
    main.querySelector('#stAdd').onclick = () => { (s.stop_actions ||= []).push({ kind: 'write', tag: '', value: false }); redraw(); };
    const parseVal = (v) => (v === 'true' ? true : v === 'false' ? false : v.trim() !== '' && !isNaN(+v) ? +v : v);
    // azioni (passi e arresto)
    main.querySelectorAll('.pr-row[data-a]').forEach((row) => {
      const arr = row.dataset.l === 'stop' ? s.stop_actions : s.steps[+row.dataset.s].actions;
      const a = arr[+row.dataset.a];
      row.querySelectorAll('[data-k]').forEach((el) => (el.onchange = () => {
        const k = el.dataset.k;
        if (k === 'kind') { for (const x of Object.keys(a)) delete a[x]; a.kind = el.value; if (el.value === 'recipe') { a.recipe_type = rtypes[0] || ''; a.recipe = '{recipe}'; } return redraw(); }
        a[k] = k === 'value' ? parseVal(el.value) : el.value;
        markDirty();
      }));
      row.querySelector('[data-del]').onclick = () => { arr.splice(+row.dataset.a, 1); redraw(); };
    });
    main.querySelectorAll('.pr-step').forEach((box) => {
      const i = +box.dataset.si, st = s.steps[i];
      box.querySelector('[data-tg]').onclick = () => { open.has(i) ? open.delete(i) : open.add(i); drawSeq(); };
      box.querySelectorAll('[data-sf]').forEach((el) => (el.onchange = () => {
        const k = el.dataset.sf, old = st.name;
        if (['min_time_s', 'timeout_s'].includes(k)) { if (el.value === '' || +el.value === 0) delete st[k]; else st[k] = +el.value; }
        else if (el.value === '' && k !== 'name') delete st[k]; else st[k] = el.value;
        if (k === 'name' && old !== st.name) {
          // aggiorna i riferimenti al passo rinominato
          for (const x of s.steps) { for (const t of x.transitions || []) if (t.next === old) t.next = st.name; if (x.on_timeout === old) x.on_timeout = st.name; }
          return redraw();
        }
        markDirty();
      }));
      box.querySelector('[data-mv="-1"]').onclick = () => { if (i > 0) { [s.steps[i - 1], s.steps[i]] = [s.steps[i], s.steps[i - 1]]; open = new Set([i - 1]); redraw(); } };
      box.querySelector('[data-mv="1"]').onclick = () => { if (i < s.steps.length - 1) { [s.steps[i + 1], s.steps[i]] = [s.steps[i], s.steps[i + 1]]; open = new Set([i + 1]); redraw(); } };
      box.querySelector('[data-ins]').onclick = () => { s.steps.splice(i + 1, 0, { name: `Passo ${s.steps.length + 1}`, transitions: [{ next: '' }] }); open = new Set([i + 1]); redraw(); };
      box.querySelector('[data-sdel]').onclick = () => { if (confirm(`Eliminare il passo ${st.name}?`)) { s.steps.splice(i, 1); open = new Set(); redraw(); } };
      if (!open.has(i)) return;
      box.querySelector('[data-aadd]').onclick = () => { (st.actions ||= []).push({ kind: 'write', tag: '', value: true }); redraw(); };
      box.querySelector('[data-tadd]').onclick = () => { (st.transitions ||= []).push({ when: [{ tag: '', op: '==', value: true }], next: '' }); redraw(); };
      box.querySelectorAll('.pr-tr').forEach((tb) => {
        const t = st.transitions[+tb.dataset.ti];
        tb.querySelectorAll('[data-tf]').forEach((el) => (el.onchange = () => { const k = el.dataset.tf; if (el.value === '' || (k === 'mode' && el.value === 'all')) delete t[k]; else t[k] = el.value; markDirty(); }));
        tb.querySelector('[data-cadd]').onclick = () => { (t.when ||= []).push({ tag: '', op: '==', value: true }); redraw(); };
        tb.querySelector('[data-tdel]').onclick = () => { st.transitions.splice(+tb.dataset.ti, 1); redraw(); };
        tb.querySelectorAll('[data-ci]').forEach((cr) => {
          const c = t.when[+cr.dataset.ci];
          cr.querySelectorAll('[data-cf]').forEach((el) => (el.onchange = () => { c[el.dataset.cf] = el.dataset.cf === 'value' && !el.value.startsWith('tag:') ? parseVal(el.value) : el.value.trim(); markDirty(); }));
          cr.querySelector('[data-cdel]').onclick = () => { t.when.splice(+cr.dataset.ci, 1); if (!t.when.length) delete t.when; redraw(); };
        });
      });
    });
  }

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
