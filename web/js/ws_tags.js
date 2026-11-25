// Workspace "Tag e UDT": tag di progetto, tipi UDT (con parametri e indirizzi
// a template), istanze UDT con anteprima degli indirizzi, trend dei tag.
import { api, esc, toast } from './core.js';
import { COMPONENTS, TREND_COLORS } from './render.js';
import { P, markDirty, onProject, areaNames, scanNames, driverNames, udtNames, TYPES } from './project.js';
import { tableEditor, modal } from './forms.js';
import { helpBtn } from './markdown.js';

const ROLES = [{ value: 'request', label: 'request (PLC)' }, { value: 'response', label: 'response (WCS)' }, { value: 'input', label: 'input (dato richiesta)' }, { value: 'output', label: 'output (risposta)' }];
const SEVERITIES = ['info', 'warning', 'alarm', 'critical'];
const OPS = ['==', '!=', '>', '>=', '<', '<='];

export function mount(root, { live }) {
  let tab = 'tags', allTags = [], selType = null, trendSel = new Set(JSON.parse(localStorage.getItem('ws.trend') || '[]')), trendMin = 10, filter = '';
  root.innerHTML = `<div class="wsl"><div class="side" id="tgSide"></div><div class="main" id="tgMain"></div></div>`;
  const side = root.querySelector('#tgSide'), main = root.querySelector('#tgMain');

  async function loadTags() { allTags = await api.get('/api/tags'); }

  function drawSide() {
    const f = filter.toLowerCase();
    const groups = { 'Tag di progetto': [], 'Da istanze UDT': [], 'File di impianto (sola lettura)': [] };
    for (const t of allTags) {
      if (f && !t.name.toLowerCase().includes(f)) continue;
      (t.editable ? groups['Tag di progetto'] : t.udt ? groups['Da istanze UDT'] : groups['File di impianto (sola lettura)']).push(t);
    }
    side.innerHTML = `<input id="tgFilter" placeholder="Cerca tag…" value="${esc(filter)}" style="width:100%">
      <p class="hint">Click su un tag per aggiungerlo al trend.</p>
      ${Object.entries(groups).filter(([, a]) => a.length).map(([g, a]) => `<details class="cat" open><summary>${g} <span class="muted">(${a.length})</span></summary>
        <ul class="list tags">${a.map((t) => `<li data-t="${esc(t.name)}" title="${esc(t.driver)} ${esc(t.address)}${t.area ? ' · area ' + esc(t.area) : ''}">
          ${trendSel.has(t.name) ? '📈 ' : ''}${esc(t.name)}<span class="tv" data-tv="${esc(t.name)}"></span></li>`).join('')}</ul></details>`).join('')}`;
    const inp = side.querySelector('#tgFilter');
    inp.oninput = () => { filter = inp.value; drawSide(); side.querySelector('#tgFilter').focus(); side.querySelector('#tgFilter').setSelectionRange(filter.length, filter.length); };
    side.querySelectorAll('[data-t]').forEach((li) => (li.onclick = () => { toggleTrend(li.dataset.t); }));
    updateValues();
  }

  function toggleTrend(n) {
    trendSel.has(n) ? trendSel.delete(n) : trendSel.add(n);
    localStorage.setItem('ws.trend', JSON.stringify([...trendSel]));
    drawSide();
    if (tab === 'trend') drawTrend(); else { tab = 'trend'; draw(); }
  }

  function updateValues() {
    root.querySelectorAll('[data-tv]').forEach((el) => { const v = live.value(el.dataset.tv); el.textContent = v === undefined ? '' : JSON.stringify(v); });
    root.querySelectorAll('[data-live-tag]').forEach((el) => {
      const st = live.tags.get(el.dataset.liveTag);
      el.textContent = st ? JSON.stringify(st.value) : '';
      el.style.color = st && st.quality !== 'good' ? 'var(--warn)' : '';
    });
  }

  const subtabs = () => `<div class="subtabs">${[['tags', 'Tag di progetto'], ['types', 'Tipi UDT'], ['inst', 'Istanze UDT'], ['trend', 'Trend']]
    .map(([k, l]) => `<button data-st="${k}" class="${tab === k ? 'on' : ''}">${l}</button>`).join('')}</div>`;

  function draw() {
    main.innerHTML = subtabs() + `<div id="tgBody"></div>`;
    main.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { tab = b.dataset.st; draw(); }));
    const body = main.querySelector('#tgBody');
    ({ tags: drawTags, types: drawTypes, inst: drawInst, trend: drawTrend })[tab](body);
  }

  // ---------------------------------------------------------------- tag
  function alarmsEditor(owner, title) {
    owner.alarms ||= [];
    const m = modal(`Allarmi – ${title}`, `<p class="hint">L'allarme scatta quando <i>valore op soglia</i> è vero. L'area è quella del tag. Nei membri UDT il messaggio può usare <code>{Instance}</code>.</p><div id="alEd"></div>`, [['Chiudi', 'primary', () => { if (!owner.alarms.length) delete owner.alarms; markDirty(); draw(); }]], { wide: true });
    tableEditor(m.querySelector('#alEd'), owner.alarms, [
      { key: 'name', label: 'Nome', width: 90, required: true }, { key: 'op', label: 'Op', type: 'select', options: OPS, empty: false },
      { key: 'value', label: 'Soglia', width: 70 }, { key: 'message', label: 'Messaggio', width: 300 },
      { key: 'severity', label: 'Gravità', type: 'select', options: SEVERITIES, empty: false },
    ], { onChange: (r) => { if (r && typeof r.value === 'string') { const n = Number(r.value); r.value = r.value === 'true' ? true : r.value === 'false' ? false : isNaN(n) ? r.value : n; } markDirty(); },
      newRow: () => ({ name: 'AL' + (owner.alarms.length + 1), op: '==', value: true, message: '', severity: 'warning' }) });
  }

  function drawTags(body) {
    body.innerHTML = `<div class="sec"><h3>Tag di progetto ${helpBtn('designer-tag#tag-di-progetto')} <span class="muted" style="font-weight:400">(${P.data.tags.length})</span></h3>
      <p class="hint">Indirizzi: S7 <code>DB10.DBW2</code> · Modbus <code>hr:12</code> · OPC UA <code>ns=2;s=…</code> · EIP nome tag Logix · sim qualsiasi chiave. Le modifiche si applicano con <b>Salva progetto</b> (riavvio a caldo dei soli driver coinvolti).</p>
      <div id="tgTbl"></div></div>`;
    tableEditor(body.querySelector('#tgTbl'), P.data.tags, [
      { key: 'name', label: 'Nome', width: 150, required: true },
      { key: 'driver', label: 'Driver', type: 'select', options: driverNames, empty: false },
      { key: 'address', label: 'Indirizzo', width: 120, mono: true, required: true },
      { key: 'type', label: 'Tipo', type: 'select', options: TYPES, empty: false },
      { key: 'len', label: 'Len', type: 'number', width: 45 },
      { key: 'writable', label: 'R/W', type: 'bool' },
      { key: 'area', label: 'Area', type: 'select', options: areaNames },
      { key: 'scan_class', label: 'Scan class', type: 'select', options: scanNames },
      { key: 'unit', label: 'Unità', width: 55 },
      { key: 'historize', label: 'Storico', type: 'bool' },
      { key: 'description', label: 'Descrizione', width: 150 },
      { key: 'al', label: 'Allarmi', type: 'button', label2: '', title: 'Allarmi del tag', onClick: (r) => alarmsEditor(r, r.name), badge: (r) => (r.alarms?.length ? ` <b>${r.alarms.length}</b>` : '') },
      { key: 'tr', label: '', type: 'button', title: 'Trend', onClick: (r) => toggleTrend(r.name) },
      { key: 'v', label: 'Valore', type: 'live', tag: (r) => r.name },
    ].map((c) => (c.type === 'button' && c.key === 'al' ? { ...c, label: '🔔' } : c.key === 'tr' ? { ...c, label: '📈' } : c)),
    { onChange: markDirty, newRow: () => ({ name: `NuovoTag${P.data.tags.length + 1}`, driver: driverNames()[0], address: '', type: 'u16' }) });
    // intestazioni leggibili per le colonne pulsante
    body.querySelectorAll('th').forEach((th) => { if (th.textContent === '🔔') th.textContent = 'Allarmi'; if (th.textContent === '📈') th.textContent = 'Trend'; });
    updateValues();
  }

  // ---------------------------------------------------------------- tipi UDT
  async function importFromLibrary() {
    const lib = await api.get('/api/library');
    const list = (lib.udts || []).filter((u) => !udtNames().includes(u.name));
    const m = modal('Importa UDT dalla libreria', list.length ? `<p class="hint">Tipi standard per famiglia di dispositivo, con layout S7 parametrico (DB, Base). Sono quelli usati dai simboli e dai faceplate di libreria.</p>
      ${list.map((u) => `<label class="row" style="margin:4px 0"><input type="checkbox" value="${esc(u.name)}"> <b>${esc(u.name)}</b> <span class="muted">${esc(u.description)} · ${u.members.length} membri</span></label>`).join('')}` : '<p>Tutti i tipi di libreria sono già nel progetto.</p>',
    [['Annulla', '', null], ['Importa', 'primary', (mm) => {
      const sel = [...mm.querySelectorAll('input:checked')].map((i) => i.value);
      for (const n of sel) P.data.udt_types.push(structuredClone(list.find((u) => u.name === n)));
      if (sel.length) { markDirty(); selType = sel[0]; draw(); toast(`${sel.length} UDT importate: ricorda Salva progetto`, 'ok'); }
    }]], { wide: true });
    return m;
  }

  function drawTypes(body) {
    const types = P.data.udt_types;
    if (!selType || !types.some((t) => t.name === selType)) selType = types[0]?.name;
    const t = types.find((x) => x.name === selType);
    body.innerHTML = `<div class="row" style="margin-bottom:10px;flex-wrap:wrap">
        ${types.map((x) => `<button class="small ${x.name === selType ? 'primary' : ''}" data-ty="${esc(x.name)}">${esc(x.name)}</button>`).join('')}
        <button class="small" id="tyNew">+ Nuovo tipo</button><button class="small" id="tyLib">⬇ Dalla libreria</button></div>
      ${t ? `<div class="sec"><h3>Tipo UDT ${helpBtn('designer-tag#tipi-udt')} <input id="tyName" value="${esc(t.name)}" style="width:200px"> <span class="grow"></span>
          <button class="small" id="tyDup">Duplica</button><button class="small danger" id="tyDel">Elimina</button></h3>
        <div class="formgrid"><label>Descrizione</label><input id="tyDesc" value="${esc(t.description || '')}"></div></div>
        <div class="sec"><h3>Parametri ${helpBtn('designer-tag#tipi-udt')}</h3><p class="hint">Usati negli indirizzi dei membri come <code>{Nome}</code> o espressioni intere <code>{Base+4}</code>, <code>{(Idx-1)*20}</code>. <code>{Instance}</code> = nome dell'istanza.</p><div id="tyParams"></div></div>
        <div class="sec"><h3>Membri ${helpBtn('designer-tag#tipi-udt')}</h3><p class="hint">Il <b>ruolo</b> definisce il protocollo request/reply quando il tipo è usato come telegramma di un punto di decisione (un membro request, uno response, input e output).</p><div id="tyMembers"></div></div>` : '<p class="muted">Nessun tipo UDT: creane uno o importalo dalla libreria.</p>'}`;
    body.querySelectorAll('[data-ty]').forEach((b) => (b.onclick = () => { selType = b.dataset.ty; draw(); }));
    body.querySelector('#tyNew').onclick = () => {
      const n = prompt('Nome del nuovo tipo UDT:');
      if (!n || types.some((x) => x.name === n)) return;
      types.push({ name: n, description: '', params: [{ name: 'Base', default: 0 }], members: [] });
      selType = n; markDirty(); draw();
    };
    body.querySelector('#tyLib').onclick = importFromLibrary;
    if (!t) return;
    body.querySelector('#tyName').onchange = (e) => {
      const n = e.target.value.trim();
      if (!n || types.some((x) => x.name === n)) return;
      for (const i of P.data.udt_instances) if (i.type === t.name) i.type = n;
      t.name = n; selType = n; markDirty(); draw();
    };
    body.querySelector('#tyDesc').onchange = (e) => { t.description = e.target.value; markDirty(); };
    body.querySelector('#tyDup').onclick = () => { const c = structuredClone(t); c.name += '_copia'; types.push(c); selType = c.name; markDirty(); draw(); };
    body.querySelector('#tyDel').onclick = () => {
      const used = P.data.udt_instances.filter((i) => i.type === t.name).map((i) => i.name);
      if (used.length) return toast(`Tipo usato dalle istanze: ${used.join(', ')}`, 'error');
      if (!confirm(`Eliminare il tipo ${t.name}?`)) return;
      types.splice(types.indexOf(t), 1); markDirty(); draw();
    };
    const params = t.params.map((p) => ({ ...p, default: p.default ?? '' }));
    tableEditor(body.querySelector('#tyParams'), params, [
      { key: 'name', label: 'Nome', width: 110, required: true }, { key: 'default', label: 'Default', width: 90 }, { key: 'description', label: 'Descrizione', width: 260 },
    ], { onChange: () => { t.params = params.map((p) => { const o = { ...p }; if (o.default === '' || o.default === undefined) delete o.default; else if (!isNaN(+o.default)) o.default = +o.default; return o; }); markDirty(); },
      newRow: () => ({ name: 'P' + (params.length + 1), default: 0 }) });
    tableEditor(body.querySelector('#tyMembers'), t.members, [
      { key: 'name', label: 'Nome', width: 110, required: true }, { key: 'type', label: 'Tipo', type: 'select', options: TYPES, empty: false },
      { key: 'address', label: 'Indirizzo (template)', width: 170, mono: true, required: true }, { key: 'len', label: 'Len', type: 'number', width: 45 },
      { key: 'writable', label: 'R/W', type: 'bool' }, { key: 'role', label: 'Ruolo protocollo', type: 'select', options: ROLES },
      { key: 'scan_class', label: 'Scan class', type: 'select', options: scanNames }, { key: 'unit', label: 'Unità', width: 55 },
      { key: 'historize', label: 'Storico', type: 'bool' }, { key: 'description', label: 'Descrizione', width: 150 },
      { key: 'al', label: '🔔', type: 'button', title: 'Allarmi del membro', onClick: (r) => alarmsEditor(r, `${t.name}.${r.name}`), badge: (r) => (r.alarms?.length ? ` <b>${r.alarms.length}</b>` : '') },
    ], { onChange: markDirty, newRow: () => ({ name: 'M' + (t.members.length + 1), type: 'u16', address: 'DB{DB}.DBW{Base+0}' }) });
  }

  // ---------------------------------------------------------------- istanze UDT
  function paramsEditor(inst) {
    const t = P.data.udt_types.find((x) => x.name === inst.type);
    if (!t) return toast('Scegli prima il tipo UDT', 'error');
    inst.params ||= {};
    const m = modal(`Parametri di ${inst.name} (${t.name})`, `<div class="formgrid">${t.params.map((p) => `<label title="${esc(p.description || '')}">${esc(p.name)}</label>
      <input data-p="${esc(p.name)}" value="${esc(inst.params[p.name] ?? '')}" placeholder="${p.default !== undefined ? 'default ' + esc(p.default) : 'obbligatorio'}">`).join('')}</div>
      <h4 style="margin:14px 0 6px">Anteprima tag generati</h4><div id="pv" class="mono" style="font-size:12px"></div>`,
    [['Chiudi', 'primary', () => { markDirty(); draw(); }]], { wide: true });
    const refresh = async () => {
      const r = await api.post('/api/project/udt_preview', { type: t, instance: inst });
      m.querySelector('#pv').innerHTML = (r.error ? `<div class="err">${esc(r.error)}</div>` : '') +
        `<table><tbody>${(r.tags || []).map((x) => `<tr><td>${esc(x.name)}</td><td>${esc(x.address)}</td><td>${esc(x.type)}</td><td>${x.writable ? 'R/W' : ''}</td><td class="muted">${esc(x.description || '')}</td></tr>`).join('')}</tbody></table>`;
    };
    m.querySelectorAll('[data-p]').forEach((i) => (i.onchange = () => {
      const v = i.value.trim();
      if (v === '') delete inst.params[i.dataset.p]; else inst.params[i.dataset.p] = isNaN(+v) ? v : +v;
      refresh();
    }));
    refresh();
  }

  function drawInst(body) {
    body.innerHTML = `<div class="sec"><h3>Istanze UDT ${helpBtn('designer-tag#istanze-udt')} <span class="muted" style="font-weight:400">(${P.data.udt_instances.length})</span></h3>
      <p class="hint">Ogni istanza genera i tag <code>Istanza.Membro</code> con gli indirizzi risolti dai parametri. Nei simboli di libreria imposta la proprietà <code>device</code> = nome istanza.</p><div id="inTbl"></div></div>`;
    tableEditor(body.querySelector('#inTbl'), P.data.udt_instances, [
      { key: 'name', label: 'Nome', width: 120, required: true },
      { key: 'type', label: 'Tipo UDT', type: 'select', options: udtNames, empty: false },
      { key: 'driver', label: 'Driver', type: 'select', options: driverNames, empty: false },
      { key: 'area', label: 'Area', type: 'select', options: areaNames },
      { key: 'scan_class', label: 'Scan class', type: 'select', options: scanNames },
      { key: 'pp', label: 'Parametri', type: 'button', title: 'Parametri e anteprima indirizzi', onClick: (r) => paramsEditor(r),
        badge: (r) => ` <span class="muted">${esc(Object.entries(r.params || {}).map(([k, v]) => `${k}=${v}`).join(' '))}</span>` },
      { key: 'description', label: 'Descrizione', width: 180 },
    ].map((c) => (c.key === 'pp' ? { ...c, label: '⚙' } : c)),
    { onChange: markDirty, newRow: () => ({ name: `DEV${P.data.udt_instances.length + 1}`, type: udtNames()[0], driver: driverNames()[0], params: {} }) });
  }

  // ---------------------------------------------------------------- trend
  function drawTrend(body = main.querySelector('#tgBody')) {
    if (!body) return;
    const names = [...trendSel];
    if (!body.querySelector('#trSvg')) {
      body.innerHTML = `<div class="sec"><h3>Trend ${helpBtn('designer-tag#trend')} <span class="grow"></span>
        <label class="row muted">Finestra <select id="trMin">${[1, 5, 10, 30, 60].map((m) => `<option ${m === trendMin ? 'selected' : ''} value="${m}">${m} min</option>`).join('')}</select></label>
        <button class="small" id="trClear">Svuota</button></h3>
        <p class="hint">Seleziona i tag dall'elenco a sinistra. Lo storico persistente è disponibile per i tag con <b>Storico</b> attivo; per gli altri il trend parte dall'apertura del Designer.</p>
        <div id="trChips"></div>
        <svg id="trSvg" viewBox="0 0 1000 380" style="width:100%;height:420px;margin-top:8px"></svg></div>`;
      body.querySelector('#trMin').onchange = (e) => { trendMin = +e.target.value; drawTrend(); };
      body.querySelector('#trClear').onclick = () => { trendSel.clear(); localStorage.setItem('ws.trend', '[]'); drawSide(); drawTrend(); };
    }
    body.querySelector('#trChips').innerHTML = names.map((n, i) => `<span class="chip" style="border-color:${TREND_COLORS[i % TREND_COLORS.length]}">${esc(n)} <a href="#" data-rm="${esc(n)}" style="color:inherit">✕</a></span>`).join('') || '<span class="muted">Nessun tag selezionato</span>';
    body.querySelectorAll('[data-rm]').forEach((a) => (a.onclick = (e) => { e.preventDefault(); toggleTrend(a.dataset.rm); }));
    live.watchHistory(names);
    body.querySelector('#trSvg').innerHTML = COMPONENTS.trend.render({ tags: names.join(','), minutes: trendMin, min: '', max: '', background: '#0a1120', legend: true }, 1000, 380, { history: (t) => live.history(t) });
  }

  let tick = 0;
  live.on('tags', () => {
    if (root.hidden) return;
    updateValues();
    if (tab === 'trend' && ++tick % 4 === 0) drawTrend();
  });
  onProject(() => { if (!root.hidden && tab !== 'trend') { /* ridisegno su modifiche esterne */ } });
  live.on('project', async () => { await loadTags(); if (!root.hidden) drawSide(); });

  (async () => { await loadTags(); drawSide(); draw(); })();
  return { show: async () => { await loadTags(); drawSide(); draw(); } };
}
