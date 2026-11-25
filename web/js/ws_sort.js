// Workspace "Smistamento": protocollo request/reply dei punti di decisione,
// regole di smistamento grafiche (con test), classi di smistamento, WMS.
import { api, esc, toast, fmtTs } from './core.js';
import { P, markDirty, areaNames, checkProject } from './project.js';
import { tableEditor, modal } from './forms.js';

const OPS = [
  ['eq', '='], ['ne', '≠'], ['gt', '>'], ['ge', '≥'], ['lt', '<'], ['le', '≤'], ['between', 'compreso tra (min..max)'],
  ['starts_with', 'inizia con'], ['ends_with', 'finisce con'], ['contains', 'contiene'], ['in', 'è uno di (a, b, c)'],
  ['not_in', 'non è uno di'], ['regex', 'regex'], ['empty', 'è vuoto'], ['not_empty', 'non è vuoto'],
  ['in_class', 'appartiene alla classe'], ['not_in_class', 'non appartiene alla classe'], ['has_class', 'ha una classe'], ['wms_known', 'il WMS conosce la destinazione'],
];
const NO_VALUE = ['empty', 'not_empty', 'has_class', 'wms_known'];

export function mount(root, { live }) {
  let tab = 'protocol', selPoint = null, rulesPoint = null, draft = null, meta = null, test = null, rulesDirty = false, tagNames = [];
  root.innerHTML = `<div class="wsl"><div class="side" id="soSide"></div><div class="main" id="soMain"></div></div>`;
  const side = root.querySelector('#soSide'), main = root.querySelector('#soMain');

  function drawSide() {
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">${[['protocol', '📡 Protocollo request/reply'], ['rules', '🔀 Regole di smistamento'], ['classes', '🏷 Classi di smistamento'], ['wms', '🔌 Connessione WMS']]
      .map(([k, l]) => `<button data-st="${k}" class="${tab === k ? 'on' : ''}" style="text-align:left">${l}</button>`).join('')}</div>
      <p class="hint">Catena di decisione: <b>regole grafiche</b> (in ordine) → se nessuna scatta, <b>funzione Python</b> del punto → altrimenti <b>fallback</b>. Le uscite <code>class:barcode</code> e <code>wms:barcode</code> usano classi e WMS.</p>`;
    side.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { if (tab === 'rules' && rulesDirty && !confirm('Regole non salvate: continuare?')) return; tab = b.dataset.st; rulesDirty = false; drawSide(); draw(); }));
  }

  function draw() {
    ({ protocol: drawProtocol, rules: drawRules, classes: drawClasses, wms: drawWms })[tab]();
  }

  // ================================================================ protocollo
  const telegramInstances = () => P.data.udt_instances.filter((i) => {
    const t = P.data.udt_types.find((x) => x.name === i.type);
    return t && t.members.some((m) => m.role === 'request') && t.members.some((m) => m.role === 'response');
  });

  async function drawProtocol() {
    const dps = P.data.decision_points;
    if (!selPoint || !dps.some((d) => d.name === selPoint)) selPoint = dps[0]?.name;
    const d = dps.find((x) => x.name === selPoint);
    main.innerHTML = `<div class="row" style="margin-bottom:10px;flex-wrap:wrap">${dps.map((x) => `<button class="small ${x.name === selPoint ? 'primary' : ''}" data-dp="${esc(x.name)}">${esc(x.name)}</button>`).join('')}
        <button class="small" id="dpNew">+ Nuovo punto di decisione</button><button class="small" id="dpTpl" title="Crea un tipo UDT telegramma con i ruoli già impostati">+ Modello telegramma</button></div>
      ${d ? `<div class="sec"><h3>Punto di decisione <input id="dpName" value="${esc(d.name)}" style="width:160px"><span class="grow"></span><button class="small danger" id="dpDel">Elimina</button></h3>
        <div class="formgrid">
          <label>Descrizione</label><input data-f="description" value="${esc(d.description || '')}">
          <label>Area</label><select data-f="area"><option value=""></option>${areaNames().map((a) => `<option ${a === d.area ? 'selected' : ''}>${esc(a)}</option>`).join('')}</select>
          <label>Definizione telegramma</label><div class="row"><label class="row"><input type="radio" name="dpm" value="udt" ${d.telegram !== undefined ? 'checked' : ''}> da istanza UDT (ruoli dei membri)</label>
            <label class="row"><input type="radio" name="dpm" value="tags" ${d.telegram === undefined ? 'checked' : ''}> tag manuali</label></div>
          ${d.telegram !== undefined ? `<label>Istanza telegramma</label><select data-f="telegram"><option value=""></option>${telegramInstances().map((i) => `<option ${i.name === d.telegram ? 'selected' : ''}>${esc(i.name)}</option>`).join('')}</select>` : `
          <label>Tag richiesta (PLC)</label><input data-f="request_tag" list="tagNames" value="${esc(d.request_tag || '')}">
          <label>Tag risposta (WCS)</label><input data-f="response_tag" list="tagNames" value="${esc(d.response_tag || '')}">`}
          <label>Handshake</label><select data-f="handshake"><option value="counter" ${d.handshake !== 'flag' ? 'selected' : ''}>Contatore (ReqID / RespID)</option><option value="flag" ${d.handshake === 'flag' ? 'selected' : ''}>Flag (Req / Ack)</option></select>
          <label>Funzione Python</label><select data-f="function"><option value="">(nessuna – solo regole)</option>${P.functions.map((f) => `<option ${f === d.function ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select>
          <label>Timeout script (ms)</label><input type="number" data-f="timeout_ms" value="${d.timeout_ms ?? 300}" style="width:120px">
        </div></div>
        ${d.telegram === undefined ? `<div class="grid2"><div class="sec"><h3>Input (PLC → WCS)</h3><div id="dpIn"></div></div><div class="sec"><h3>Output (WCS → PLC)</h3><div id="dpOut"></div></div></div>` : ''}
        <div class="sec"><h3>Fallback</h3><p class="hint">Uscite scritte se nessuna regola/script fornisce una risposta valida. Se vuoto, il WCS non risponde e il PLC applica il proprio default al timeout.</p><div id="dpFb"></div></div>
        <div class="sec"><h3>Protocollo risolto</h3><div id="dpRes" class="mono" style="font-size:12.5px">verifica…</div></div>
        <div class="sec"><h3>Sequenza dell'handshake</h3>${handshakeDiagram(d.handshake || 'counter')}</div>` : '<p class="muted">Nessun punto di decisione.</p>'}`;
    main.querySelectorAll('[data-dp]').forEach((b) => (b.onclick = () => { selPoint = b.dataset.dp; drawProtocol(); }));
    main.querySelector('#dpNew').onclick = () => {
      const n = prompt('Nome del punto di decisione (es. SC2):');
      if (!n || dps.some((x) => x.name === n)) return;
      dps.push({ name: n, telegram: telegramInstances()[0]?.name || '', handshake: 'counter', timeout_ms: 300 });
      selPoint = n; markDirty(); drawProtocol();
    };
    main.querySelector('#dpTpl').onclick = () => {
      let name = 'Telegramma', i = 1;
      while (P.data.udt_types.some((t) => t.name === name)) name = `Telegramma${++i}`;
      P.data.udt_types.push({ name, description: 'Telegramma request/reply (S7): dati + ReqID dal PLC, uscite + RespID dal WCS', params: [{ name: 'DB', default: 100 }, { name: 'Base', default: 0 }],
        members: [
          { name: 'ReqID', type: 'u16', address: 'DB{DB}.DBW{Base+0}', role: 'request', description: 'Contatore richiesta (PLC)' },
          { name: 'Barcode', type: 'string', len: 20, address: 'DB{DB}.DBB{Base+2}', role: 'input', description: 'Barcode letto' },
          { name: 'Weight', type: 'f32', address: 'DB{DB}.DBD{Base+24}', role: 'input', description: 'Peso (kg)' },
          { name: 'RespID', type: 'u16', address: 'DB{DB}.DBW{Base+28}', writable: true, role: 'response', description: 'Eco ReqID (WCS)' },
          { name: 'Destination', type: 'u16', address: 'DB{DB}.DBW{Base+30}', writable: true, role: 'output', description: 'Uscita assegnata' },
          { name: 'Result', type: 'u16', address: 'DB{DB}.DBW{Base+32}', writable: true, role: 'output', description: 'Esito' },
        ] });
      markDirty();
      toast(`Creato il tipo UDT "${name}": personalizzalo in Tag e UDT → Tipi UDT e crea un'istanza`, 'ok');
    };
    if (!d) return;
    const fix = () => { markDirty(); };
    main.querySelector('#dpName').onchange = (e) => { const n = e.target.value.trim(); if (n && !dps.some((x) => x.name === n)) { d.name = n; selPoint = n; fix(); drawProtocol(); } };
    main.querySelector('#dpDel').onclick = () => { if (confirm(`Eliminare il punto ${d.name}?`)) { dps.splice(dps.indexOf(d), 1); fix(); drawProtocol(); } };
    main.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
      const k = el.dataset.f, v = el.type === 'number' ? +el.value : el.value;
      if (v === '' && k !== 'telegram') delete d[k]; else d[k] = v;
      fix();
      if (k === 'handshake' || k === 'telegram') drawProtocol(); else resolve();
    }));
    main.querySelectorAll('[name=dpm]').forEach((r) => (r.onchange = () => {
      if (r.value === 'udt') { d.telegram = telegramInstances()[0]?.name || ''; delete d.request_tag; delete d.response_tag; delete d.inputs; delete d.outputs; }
      else { delete d.telegram; d.inputs ||= {}; d.outputs ||= {}; }
      fix(); drawProtocol();
    }));
    const mapEd = (el, key) => {
      d[key] ||= {};
      const rows = Object.entries(d[key]).map(([alias, tag]) => ({ alias, tag }));
      tableEditor(el, rows, [{ key: 'alias', label: 'Alias (campo regole)', width: 120 }, { key: 'tag', label: 'Tag', width: 200, list: 'tagNames' }],
        { onChange: () => { d[key] = Object.fromEntries(rows.filter((r) => r.alias && r.tag).map((r) => [r.alias, r.tag])); fix(); resolve(); }, newRow: () => ({ alias: '', tag: '' }) });
    };
    if (main.querySelector('#dpIn')) { mapEd(main.querySelector('#dpIn'), 'inputs'); mapEd(main.querySelector('#dpOut'), 'outputs'); }
    async function resolve() {
      const r = await checkProject();
      const box = main.querySelector('#dpRes');
      if (!box) return;
      if (!r.ok) { box.innerHTML = `<div class="err">${esc(r.error)}</div>`; drawFallback([]); return; }
      const x = r.decision_points.find((p) => p.name === d.name);
      if (!x) { box.textContent = '—'; return; }
      box.innerHTML = `<div>richiesta: <b>${esc(x.request_tag)}</b> · risposta: <b>${esc(x.response_tag)}</b></div>
        <div>input: ${Object.entries(x.inputs).map(([a, t]) => `<span class="chip">${esc(a)} ← ${esc(t)}</span>`).join('') || '—'}</div>
        <div>output: ${Object.entries(x.outputs).map(([a, t]) => `<span class="chip">${esc(a)} → ${esc(t)}</span>`).join('') || '—'}</div>`;
      drawFallback(Object.keys(x.outputs));
    }
    function drawFallback(outs) {
      const el = main.querySelector('#dpFb');
      if (!el) return;
      d.fallback ||= {};
      el.innerHTML = outs.length ? `<div class="formgrid">${outs.map((o) => `<label>${esc(o)}</label><input data-fb="${esc(o)}" value="${esc(d.fallback[o] ?? '')}" placeholder="(nessuno)" style="width:140px">`).join('')}</div>` : '<span class="muted">Nessuna uscita.</span>';
      el.querySelectorAll('[data-fb]').forEach((i) => (i.onchange = () => {
        const v = i.value.trim();
        if (v === '') delete d.fallback[i.dataset.fb]; else d.fallback[i.dataset.fb] = isNaN(+v) ? v : +v;
        if (!Object.keys(d.fallback).length) delete d.fallback;
        fix();
      }));
    }
    resolve();
  }

  function handshakeDiagram(mode) {
    const steps = mode === 'flag'
      ? [['PLC', 'scrive dati (barcode, peso…)'], ['PLC', 'Req = 1'], ['WCS', 'regole → script → fallback'], ['WCS', 'scrive uscite'], ['WCS', 'Ack = 1'], ['PLC', 'legge uscite, Req = 0'], ['WCS', 'Ack = 0']]
      : [['PLC', 'scrive dati (barcode, peso…)'], ['PLC', 'ReqID = ReqID + 1 (≠ 0)'], ['WCS', 'rileva ReqID ≠ RespID'], ['WCS', 'regole → script → fallback'], ['WCS', 'scrive uscite'], ['WCS', 'RespID = ReqID'], ['PLC', 'uscite valide se RespID = ReqID (timeout → default PLC)']];
    let y = 40, h = `<svg class="flowdiag" viewBox="0 0 760 ${60 + steps.length * 42}"><text x="150" y="20" fill="#93c5fd" font-size="14" text-anchor="middle" font-weight="700">PLC</text><text x="610" y="20" fill="#86efac" font-size="14" text-anchor="middle" font-weight="700">WCS</text>
      <line x1="150" y1="30" x2="150" y2="${50 + steps.length * 42}" stroke="#3b82f6" stroke-dasharray="4 4"/><line x1="610" y1="30" x2="610" y2="${50 + steps.length * 42}" stroke="#22c55e" stroke-dasharray="4 4"/>`;
    steps.forEach(([who, txt], i) => {
      const x = who === 'PLC' ? 150 : 610, col = who === 'PLC' ? '#3b82f6' : '#22c55e';
      h += `<circle cx="${x}" cy="${y}" r="6" fill="${col}"/><text x="${who === 'PLC' ? x + 14 : x - 14}" y="${y + 4}" fill="#e2e8f0" font-size="13" text-anchor="${who === 'PLC' ? 'start' : 'end'}">${i + 1}. ${esc(txt)}</text>`;
      if (i < steps.length - 1 && steps[i + 1][0] !== who) {
        const x2 = who === 'PLC' ? 610 : 150;
        h += `<line x1="${x}" y1="${y + 8}" x2="${x2}" y2="${y + 34}" stroke="#64748b" marker-end="url(#ar)"/>`;
      }
      y += 42;
    });
    return h + `<defs><marker id="ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0L10 5L0 10z" fill="#64748b"/></marker></defs></svg>`;
  }

  // ================================================================ regole
  async function drawRules() {
    const points = await api.get('/api/rules');
    tagNames = (await api.get('/api/tags')).map((t) => t.name);
    if (!rulesPoint || !points.some((p) => p.name === rulesPoint)) rulesPoint = points[0]?.name;
    if (!rulesPoint) { main.innerHTML = '<p class="muted">Nessun punto di decisione salvato. Crealo nella sezione Protocollo e premi Salva progetto.</p>'; return; }
    if (!draft || draft._point !== rulesPoint) await loadRules();
    renderRules(points);
  }

  async function loadRules() {
    const r = await api.get(`/api/rules/${encodeURIComponent(rulesPoint)}`);
    meta = r;
    draft = structuredClone(r.ruleset);
    draft._point = rulesPoint;
    draft.rules ||= [];
    test = null;
    rulesDirty = false;
  }

  const newId = () => 'r' + Math.random().toString(36).slice(2, 7);

  function fieldOptions() {
    const ins = Object.keys(meta.point.inputs || {});
    return `<datalist id="ruleFields">${[...ins, 'request_id'].map((f) => `<option value="${esc(f)}">`).join('')}${tagNames.map((t) => `<option value="tag:${esc(t)}">`).join('')}</datalist>
      <datalist id="ruleClasses">${P.data.sort_classes.map((c) => `<option value="${esc(c.name)}">`).join('')}</datalist>
      <datalist id="ruleOuts">${Object.keys(meta.point.inputs || {}).flatMap((f) => [`class:${f}`, `wms:${f}`]).map((v) => `<option value="${esc(v)}">`).join('')}${tagNames.map((t) => `<option value="tag:${esc(t)}">`).join('')}</datalist>`;
  }

  function actionHtml(a, prefix) {
    const outs = Object.keys(meta.point.outputs || {});
    return `${outs.map((o) => `<div class="outrow"><label class="muted">${esc(o)}</label><input data-${prefix}out="${esc(o)}" list="ruleOuts" value="${esc(a.outputs?.[o] ?? '')}" placeholder="(fallback) · es. 3 · 1|2 · class:barcode · wms:barcode"></div>`).join('')}
      <div class="outrow"><label class="muted">script</label><select data-${prefix}script><option value="">(nessuno)</option>${(meta.functions || []).map((f) => `<option ${f === a.script ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select></div>
      <div class="outrow"><label class="muted">log</label><input data-${prefix}log value="${esc(a.log || '')}" placeholder="testo, es. {barcode} → uscita 2"></div>`;
  }

  function renderRules(points) {
    const hits = meta.hits || {};
    const trace = test?.trace || [];
    const hitId = test?.outcome?.rule;
    main.innerHTML = fieldOptions() + `<div class="row" style="margin-bottom:10px;flex-wrap:wrap">
        <label class="row muted">Punto di decisione <select id="rpSel">${points.map((p) => `<option ${p.name === rulesPoint ? 'selected' : ''}>${esc(p.name)}</option>`).join('')}</select></label>
        <label class="row"><input type="checkbox" id="rsEn" ${draft.enabled !== false ? 'checked' : ''}> regole attive</label>
        <span class="grow"></span>
        <button class="small" id="rsReset">Azzera contatori</button><button class="small" id="rsReload">Annulla modifiche</button>
        <button class="primary" id="rsSave">Salva regole${rulesDirty ? ' ●' : ''}</button></div>
      <div class="row" style="align-items:flex-start;gap:16px">
        <div class="grow" style="min-width:0">
          <input id="rsDesc" value="${esc(draft.description || '')}" placeholder="Descrizione dell'insieme di regole" style="width:100%;margin-bottom:10px">
          ${draft.rules.map((r, i) => {
            const tr = trace.find((t) => t.id === r.id);
            return `<div class="rule ${r.enabled === false ? 'off' : ''} ${hitId === r.id ? 'hit' : ''}" data-ri="${i}">
            <div class="rh"><b class="muted">#${i + 1}</b><input type="checkbox" data-en ${r.enabled !== false ? 'checked' : ''} title="Abilitata">
              <input data-name value="${esc(r.name || '')}" placeholder="Nome regola" style="flex:1">
              <select data-mode title="Combinazione condizioni"><option value="all" ${r.mode !== 'any' ? 'selected' : ''}>TUTTE le condizioni</option><option value="any" ${r.mode === 'any' ? 'selected' : ''}>ALMENO UNA</option></select>
              <span class="hits" title="Colli decisi da questa regola dall'avvio">▶ ${hits[r.id] || 0}</span>
              <button class="small" data-up title="Su">↑</button><button class="small" data-down title="Giù">↓</button><button class="small" data-dup title="Duplica">⧉</button><button class="small danger" data-del>✕</button></div>
            <div class="rb"><div><h5>Se</h5>${(r.when || []).map((c, k) => `<div class="cond" data-ci="${k}">
                <input data-cf list="ruleFields" value="${esc(c.field)}" placeholder="campo">
                <select data-co>${OPS.map(([v, l]) => `<option value="${v}" ${v === c.op ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>
                <input data-cv value="${esc(c.value || '')}" ${NO_VALUE.includes(c.op) ? 'disabled placeholder="—"' : `placeholder="valore o tag:Nome" ${['in_class', 'not_in_class'].includes(c.op) ? 'list="ruleClasses"' : ''}`}>
                <span class="res ${tr ? (tr.conditions[k] ? 't' : 'f') : ''}" title="Esito nell'ultimo test"></span>
                <button class="small danger" data-cdel>✕</button></div>`).join('')}
                <button class="small" data-cadd>+ Condizione</button></div>
              <div><h5>Allora</h5>${actionHtml(r.then || {}, 'r')}</div></div></div>`;
          }).join('')}
          <button id="rAdd">+ Nuova regola</button>
          <div class="rule ${hitId === 'default' ? 'hit' : ''}" style="margin-top:14px"><div class="rh"><b>Default</b> <span class="muted">se nessuna regola scatta</span><span class="grow"></span>
            <label class="row"><input type="checkbox" id="defEn" ${draft.default ? 'checked' : ''}> attivo</label><span class="hits">▶ ${hits.default || 0}</span></div>
            ${draft.default ? `<div class="rb"><div class="muted" style="font-size:12.5px">Se il default non è attivo: funzione Python <b>${esc(meta.point.function || '—')}</b>, poi fallback ${esc(JSON.stringify(meta.point.fallback || {}))}.</div><div>${actionHtml(draft.default, 'd')}</div></div>` : ''}</div>
        </div>
        <div style="width:330px;flex:none;position:sticky;top:0">
          <div class="sec"><h3>Prova regole</h3><p class="hint">Valuta la bozza corrente (anche non salvata) con dati di esempio e tag live. Nessuna scrittura al PLC.</p>
            ${Object.keys(meta.point.inputs || {}).map((f) => `<div class="outrow"><label class="muted">${esc(f)}</label><input data-tin="${esc(f)}" value="${esc(test?.inputs?.[f] ?? '')}"></div>`).join('')}
            <button class="primary" id="rTest" style="width:100%;margin-top:6px">▶ Prova</button>
            <div id="rTestOut" style="margin-top:10px">${test ? testHtml() : ''}</div></div>
          <div class="sec"><h3>Ultime decisioni</h3><div id="rLast" style="font-size:12px">…</div></div>
        </div></div>`;
    wireRules(points);
    api.get(`/api/decisions?point=${encodeURIComponent(rulesPoint)}&limit=8`).then((ds) => {
      const el = main.querySelector('#rLast');
      if (el) el.innerHTML = ds.map((x) => `<div style="margin-bottom:4px"><span class="muted">${fmtTs(x.ts).split(' ')[1]}</span> ${esc(Object.values(x.inputs)[0] ?? '')} → <b>${esc(JSON.stringify(x.outputs))}</b> <span class="muted">${esc(x.rule || x.status)}</span></div>`).join('') || '<span class="muted">nessuna</span>';
    });
  }

  function testHtml() {
    if (test.error) return `<div class="err">${esc(test.error)}</div>`;
    const o = test.outcome;
    return `${o ? `<div>Regola: <b style="color:var(--ok)">${esc(o.rule_name || o.rule)}</b></div><div>Uscite: <b class="mono">${esc(JSON.stringify(o.outputs))}</b></div>${o.script ? `<div>Delegata a script: ${esc(o.script)}</div>` : ''}${o.log ? `<div class="muted">log: ${esc(o.log)}</div>` : ''}`
      : `<div style="color:var(--warn)">Nessuna regola: ${test.function ? `script <b>${esc(test.function)}</b>` : 'fallback ' + esc(JSON.stringify(test.fallback))}</div>`}
      <div class="muted" style="margin-top:4px">Classi: ${(test.classes || []).map((c) => `<span class="chip">${esc(c)}</span>`).join('') || '—'}</div>`;
  }

  function changed(redraw = false) {
    rulesDirty = true;
    const b = main.querySelector('#rsSave');
    if (b) b.textContent = 'Salva regole ●';
    if (redraw) renderRules(lastPoints);
  }
  let lastPoints = [];

  function readAction(scopeEl, prefix, a) {
    a.outputs = {};
    scopeEl.querySelectorAll(`[data-${prefix}out]`).forEach((i) => { if (i.value.trim()) a.outputs[i.getAttribute(`data-${prefix}out`)] = i.value.trim(); });
    const sc = scopeEl.querySelector(`[data-${prefix}script]`).value;
    if (sc) a.script = sc; else delete a.script;
    const lg = scopeEl.querySelector(`[data-${prefix}log]`).value;
    if (lg) a.log = lg; else delete a.log;
  }

  function wireRules(points) {
    lastPoints = points;
    main.querySelector('#rpSel').onchange = async (e) => {
      if (rulesDirty && !confirm('Regole non salvate: continuare?')) { e.target.value = rulesPoint; return; }
      rulesPoint = e.target.value; await loadRules(); renderRules(points);
    };
    main.querySelector('#rsEn').onchange = (e) => { draft.enabled = e.target.checked; changed(); };
    main.querySelector('#rsDesc').onchange = (e) => { draft.description = e.target.value; changed(); };
    main.querySelector('#rsReload').onclick = async () => { await loadRules(); renderRules(points); };
    main.querySelector('#rsReset').onclick = async () => { await api.post(`/api/rules/${encodeURIComponent(rulesPoint)}/reset`); meta = await api.get(`/api/rules/${encodeURIComponent(rulesPoint)}`); renderRules(points); };
    main.querySelector('#rsSave').onclick = async () => {
      try {
        const body = structuredClone(draft); delete body._point;
        await api.put(`/api/rules/${encodeURIComponent(rulesPoint)}`, body);
        toast('Regole salvate e attive', 'ok');
        await loadRules(); renderRules(points);
      } catch (e) { toast(`Regole NON salvate:\n${e.message}`, 'error'); }
    };
    main.querySelectorAll('[data-ri]').forEach((box) => {
      const i = +box.dataset.ri, r = draft.rules[i];
      box.querySelector('[data-en]').onchange = (e) => { r.enabled = e.target.checked; changed(true); };
      box.querySelector('[data-name]').onchange = (e) => { r.name = e.target.value; changed(); };
      box.querySelector('[data-mode]').onchange = (e) => { r.mode = e.target.value; changed(); };
      box.querySelector('[data-up]').onclick = () => { if (i > 0) { [draft.rules[i - 1], draft.rules[i]] = [r, draft.rules[i - 1]]; changed(true); } };
      box.querySelector('[data-down]').onclick = () => { if (i < draft.rules.length - 1) { [draft.rules[i + 1], draft.rules[i]] = [r, draft.rules[i + 1]]; changed(true); } };
      box.querySelector('[data-dup]').onclick = () => { const c = structuredClone(r); c.id = newId(); c.name = (c.name || '') + ' (copia)'; draft.rules.splice(i + 1, 0, c); changed(true); };
      box.querySelector('[data-del]').onclick = () => { if (confirm(`Eliminare la regola "${r.name || r.id}"?`)) { draft.rules.splice(i, 1); changed(true); } };
      box.querySelector('[data-cadd]').onclick = () => { (r.when ||= []).push({ field: Object.keys(meta.point.inputs || {})[0] || '', op: 'eq', value: '' }); changed(true); };
      box.querySelectorAll('[data-ci]').forEach((row) => {
        const c = r.when[+row.dataset.ci];
        row.querySelector('[data-cf]').onchange = (e) => { c.field = e.target.value.trim(); changed(); };
        row.querySelector('[data-co]').onchange = (e) => { c.op = e.target.value; if (NO_VALUE.includes(c.op)) c.value = ''; changed(true); };
        row.querySelector('[data-cv]').onchange = (e) => { c.value = e.target.value; changed(); };
        row.querySelector('[data-cdel]').onclick = () => { r.when.splice(+row.dataset.ci, 1); changed(true); };
      });
      box.querySelectorAll('[data-rout],[data-rscript],[data-rlog]').forEach((el) => (el.onchange = () => { readAction(box, 'r', (r.then ||= {})); changed(); }));
    });
    main.querySelector('#rAdd').onclick = () => {
      const f = Object.keys(meta.point.inputs || {})[0] || '';
      draft.rules.push({ id: newId(), name: 'Nuova regola', enabled: true, mode: 'all', when: [{ field: f, op: 'starts_with', value: '' }], then: { outputs: {} } });
      changed(true);
    };
    main.querySelector('#defEn').onchange = (e) => { if (e.target.checked) draft.default = { outputs: {} }; else delete draft.default; changed(true); };
    const defBox = main.querySelector('#defEn').closest('.rule');
    defBox.querySelectorAll('[data-dout],[data-dscript],[data-dlog]').forEach((el) => (el.onchange = () => { readAction(defBox, 'd', draft.default); changed(); }));
    main.querySelector('#rTest').onclick = async () => {
      const inputs = {};
      main.querySelectorAll('[data-tin]').forEach((i) => { const v = i.value; inputs[i.dataset.tin] = v !== '' && !isNaN(+v) ? +v : v; });
      try {
        const body = structuredClone(draft); delete body._point;
        test = { ...(await api.post(`/api/rules/${encodeURIComponent(rulesPoint)}/test`, { inputs, ruleset: body })), inputs };
      } catch (e) { test = { error: e.message, inputs }; }
      renderRules(points);
    };
  }

  // ================================================================ classi
  function drawClasses() {
    main.innerHTML = `<div class="sec"><h3>Classi di smistamento</h3>
      <p class="hint">Una classe raggruppa barcode (elenco esplicito e/o pattern con <code>*</code> e <code>?</code>) e ha una destinazione (<code>3</code> oppure <code>1|2</code> a rotazione).
      Le classi con origine <b>WMS</b> ricevono l'elenco dei barcode dal WMS (sincronizzazione periodica). Nelle regole: condizione <i>appartiene alla classe</i>, uscita <code>class:barcode</code>.</p>
      <div id="clTbl"></div></div>
      <div class="sec"><h3>Prova un barcode</h3><div class="row"><input id="clBc" placeholder="barcode" style="width:220px"><label class="row"><input type="checkbox" id="clWms" checked> chiedi anche al WMS</label><button id="clGo">Prova</button></div>
      <div id="clRes" style="margin-top:8px"></div><p class="hint">La prova usa il progetto <b>salvato</b>.</p></div>`;
    tableEditor(main.querySelector('#clTbl'), P.data.sort_classes, [
      { key: 'name', label: 'Nome', width: 120, required: true }, { key: 'description', label: 'Descrizione', width: 170 },
      { key: 'destination', label: 'Destinazione', width: 80 },
      { key: 'source', label: 'Origine', type: 'select', options: [{ value: 'local', label: 'WCS (locale)' }, { value: 'wms', label: 'WMS' }], empty: false },
      { key: 'patterns', label: 'Pattern', type: 'list', width: 220, placeholder: 'IT-MI*, ??-PA-*' },
      { key: 'barcodes', label: 'Barcode espliciti', type: 'list', width: 220, placeholder: 'ABC123, ABC124' },
    ], { onChange: markDirty, newRow: () => ({ name: `CLASSE${P.data.sort_classes.length + 1}`, destination: '1', source: 'local', patterns: [] }) });
    main.querySelector('#clGo').onclick = async () => {
      const bc = main.querySelector('#clBc').value.trim();
      if (!bc) return;
      const r = await api.get(`/api/sorting/lookup?barcode=${encodeURIComponent(bc)}&wms=${main.querySelector('#clWms').checked}`);
      main.querySelector('#clRes').innerHTML = `<div>Classi: ${r.classes.map((c) => `<span class="chip ok">${esc(c)}</span>`).join('') || '<span class="muted">nessuna</span>'}</div>
        <div>Destinazione da classe: <b>${esc(r.class ? r.class[1] : '—')}</b></div>
        ${r.wms ? `<div>WMS: ${r.wms.error ? `<span class="err">${esc(r.wms.error)}</span>` : `destinazione <b>${esc(r.wms.destination ?? '— (sconosciuto)')}</b>${r.wms.class ? `, classe ${esc(r.wms.class)}` : ''}`}</div>` : ''}`;
    };
  }

  // ================================================================ WMS
  async function drawWms() {
    const w = P.data.wms;
    const st = (await api.get('/api/sorting/status')).wms;
    const F = (k, l, ph = '', type = 'text') => `<label>${l}</label><input data-w="${k}" type="${type}" value="${esc(w?.[k] ?? '')}" placeholder="${esc(ph)}">`;
    main.innerHTML = `<div class="sec"><h3>Connessione WMS <span class="grow"></span><label class="row"><input type="checkbox" id="wEn" ${w?.enabled !== false && w ? 'checked' : ''}> abilitata</label></h3>
      ${w ? `<div class="formgrid">
        ${F('base_url', 'URL base', 'http://wms.local/api')}
        ${F('destination_path', 'Destinazione collo (GET)', '/parcels/{barcode}/destination')}
        ${F('destination_field', 'Campo destinazione', 'destination')}
        ${F('class_field', 'Campo classe', 'class')}
        ${F('classes_path', 'Elenco classi (GET)', '/sort-classes')}
        ${F('sync_interval_s', 'Sincronizzazione classi (s)', '60', 'number')}
        ${F('timeout_ms', 'Timeout richiesta (ms)', '250', 'number')}
        ${F('cache_ttl_s', 'Cache risposte (s)', '300', 'number')}
        <label>Header HTTP</label><div id="wHdr"></div></div>
        <p class="hint">Il WMS risponde con JSON; un 404 significa "collo sconosciuto" (la regola che usa <code>wms:barcode</code> viene saltata). Per i token usa <code>\${NOME_VARIABILE}</code>: il valore è letto dall'ambiente del server, non salvato nel progetto.</p>` : '<p class="muted">WMS non configurato: abilitalo per impostare la connessione.</p>'}</div>
      <div class="sec"><h3>Stato</h3><div class="formgrid">
        <label>Stato</label><div>${st.enabled ? '<span class="chip ok">abilitato</span>' : '<span class="chip">disabilitato</span>'} ${st.last_error ? `<span class="chip err">${esc(st.last_error)}</span>` : ''}</div>
        <label>Richieste / errori</label><div>${st.requests} / ${st.errors} · cache ${st.cache_size}</div>
        <label>Ultima sincronizzazione</label><div>${st.last_sync ? fmtTs(st.last_sync) : '—'} ${st.synced_classes.map((c) => `<span class="chip">${esc(c)}</span>`).join('')}</div></div>
        <button id="wSync" style="margin-top:8px">Sincronizza classi ora</button> <span class="hint">(usa il progetto salvato)</span></div>`;
    main.querySelector('#wEn').onchange = (e) => {
      if (e.target.checked) P.data.wms = { ...(w || { base_url: 'http://127.0.0.1:8090/api' }), enabled: true };
      else if (P.data.wms) P.data.wms.enabled = false;
      markDirty(); drawWms();
    };
    main.querySelectorAll('[data-w]').forEach((i) => (i.onchange = () => {
      const v = i.type === 'number' ? (i.value === '' ? undefined : +i.value) : i.value;
      if (v === undefined || v === '') delete w[i.dataset.w]; else w[i.dataset.w] = v;
      markDirty();
    }));
    if (w) {
      w.headers ||= {};
      const rows = Object.entries(w.headers).map(([k, v]) => ({ k, v }));
      tableEditor(main.querySelector('#wHdr'), rows, [{ key: 'k', label: 'Header', width: 140 }, { key: 'v', label: 'Valore', width: 220 }],
        { onChange: () => { w.headers = Object.fromEntries(rows.filter((r) => r.k).map((r) => [r.k, r.v || ''])); if (!Object.keys(w.headers).length) delete w.headers; markDirty(); }, newRow: () => ({ k: 'Authorization', v: 'Bearer ${WMS_TOKEN}' }), reorder: false });
    }
    main.querySelector('#wSync').onclick = async () => {
      try { const r = await api.post('/api/sorting/sync'); toast(`Classi sincronizzate: ${r.synced}`, 'ok'); } catch (e) { toast(e.message, 'error'); }
      drawWms();
    };
  }

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
