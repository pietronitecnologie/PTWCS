// Workspace "Progetto": aree (gerarchia), scan class, script a timer, driver.
import { api, esc } from './core.js';
import { P, markDirty, areaNames } from './project.js';
import { tableEditor } from './forms.js';

export function mount(root, { live }) {
  let tab = 'areas';
  root.innerHTML = `<div class="wsl"><div class="side" id="pjSide"></div><div class="main" id="pjMain"></div></div>`;
  const side = root.querySelector('#pjSide'), main = root.querySelector('#pjMain');

  function drawSide() {
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">${[['areas', '🗺 Aree'], ['scan', '⏱ Scan class'], ['timers', '⏲ Script a timer'], ['wh', '🏬 Magazzini'], ['drivers', '🔌 Driver PLC']]
      .map(([k, l]) => `<button data-st="${k}" class="${tab === k ? 'on' : ''}" style="text-align:left">${l}</button>`).join('')}</div>
      <p class="hint">Le modifiche sono applicate a caldo con <b>Salva progetto</b>. I driver sono definiti nel file di impianto (<code>plant.yaml</code>).</p>`;
    side.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { tab = b.dataset.st; drawSide(); draw(); }));
  }

  function draw() {
    if (tab === 'areas') {
      main.innerHTML = `<div class="sec"><h3>Aree</h3><p class="hint">Ogni tag (o istanza UDT, punto di decisione, driver) appartiene a un'area: gli allarmi del tag, gli errori di qualità e la perdita di comunicazione finiscono nell'area di appartenenza. Le aree possono essere annidate (l'area padre comprende le figlie).</p><div id="arTbl"></div></div>
        <div class="sec"><h3>Gerarchia</h3><div id="arTree"></div></div>`;
      const tree = () => {
        const kids = (p) => P.data.areas.filter((a) => (a.parent || '') === p);
        const rec = (p, d) => kids(p).map((a) => `<div style="padding-left:${d * 18}px">▸ <b>${esc(a.name)}</b> <span class="muted">${esc(a.description || '')}</span></div>${rec(a.name, d + 1)}`).join('');
        main.querySelector('#arTree').innerHTML = rec('', 0) || '<span class="muted">Nessuna area</span>';
      };
      tableEditor(main.querySelector('#arTbl'), P.data.areas, [
        { key: 'name', label: 'Nome', width: 160, required: true }, { key: 'description', label: 'Descrizione', width: 280 },
        { key: 'parent', label: 'Area padre', type: 'select', options: areaNames },
      ], { onChange: () => { markDirty(); tree(); }, newRow: () => ({ name: `Area${P.data.areas.length + 1}` }) });
      tree();
    } else if (tab === 'scan') {
      main.innerHTML = `<div class="sec"><h3>Scan class</h3><p class="hint">Frequenze di lettura assegnabili a tag, membri UDT e istanze. I tag senza scan class usano <code>poll_ms</code> del driver. Nello stesso ciclo i gruppi in scadenza sono letti insieme (letture a blocchi ottimizzate).</p><div id="scTbl"></div></div>`;
      tableEditor(main.querySelector('#scTbl'), P.data.scan_classes, [
        { key: 'name', label: 'Nome', width: 140, required: true }, { key: 'rate_ms', label: 'Periodo (ms)', type: 'number', width: 100, required: true },
        { key: 'description', label: 'Descrizione', width: 300 },
      ], { onChange: markDirty, newRow: () => ({ name: `scan${P.data.scan_classes.length + 1}`, rate_ms: 1000 }) });
    } else if (tab === 'timers') {
      main.innerHTML = `<div class="sec"><h3>Script a timer</h3><p class="hint">Funzioni Python <code>def f(payload, ctx)</code> eseguite periodicamente nel processo HMI (separato dalle decisioni in tempo reale). Utili per logiche di supporto, calcoli KPI, simulazioni (<code>sim_magazzino</code> anima la demo).</p><div id="tmTbl"></div></div>`;
      tableEditor(main.querySelector('#tmTbl'), P.data.timer_scripts, [
        { key: 'name', label: 'Nome', width: 160, required: true },
        { key: 'function', label: 'Funzione', type: 'select', options: () => P.functions, empty: false },
        { key: 'interval_ms', label: 'Intervallo (ms)', type: 'number', width: 100, required: true },
        { key: 'enabled', label: 'Attivo', type: 'bool' },
      ], { onChange: markDirty, newRow: () => ({ name: `timer${P.data.timer_scripts.length + 1}`, function: P.functions[0] || '', interval_ms: 1000, enabled: true }) });
    } else if (tab === 'wh') {
      drawWh();
    } else {
      main.innerHTML = `<div class="sec"><h3>Driver PLC</h3><p class="hint">Definiti in <code>plant.yaml</code> (sola lettura qui). Stato aggiornato in tempo reale.</p><div id="drv"></div></div>`;
      const drawDrv = () => {
        const st = Object.fromEntries((live.status?.drivers || []).map((d) => [d.name, d]));
        const el = main.querySelector('#drv');
        if (!el) return;
        el.innerHTML = `<table><thead><tr><th>Nome</th><th>Protocollo</th><th>Area</th><th>Stato</th><th>Ciclo</th><th>Letture</th><th>Scritture</th><th>Errori</th></tr></thead><tbody>
          ${P.drivers.map((d) => { const s = st[d.name] || {}; return `<tr><td>${esc(d.name)}</td><td>${esc(d.protocol)}</td><td>${esc(d.area || '')}</td>
            <td>${s.connected ? '<span class="chip ok">connesso</span>' : `<span class="chip err" title="${esc(s.last_error || '')}">disconnesso</span>`}</td>
            <td>${s.cycle_ms?.toFixed?.(1) ?? '-'} ms</td><td>${s.reads ?? '-'}</td><td>${s.writes ?? '-'}</td><td>${s.errors ?? '-'}</td></tr>`; }).join('')}</tbody></table>`;
      };
      drawDrv();
      live.on('status', () => { if (tab === 'drivers' && !root.hidden) drawDrv(); });
    }
  }

  // ------------------------------------------------------------ magazzini automatici
  let whSel = 0;
  function drawWh() {
    P.data.warehouses = P.data.warehouses || [];
    const list = P.data.warehouses;
    if (whSel >= list.length) whSel = 0;
    const w = list[whSel];
    main.innerHTML = `<div class="sec"><h3>Magazzini automatici
        <select id="whSel" style="margin-left:8px">${list.map((x, i) => `<option value="${i}" ${i === whSel ? 'selected' : ''}>${esc(x.name)}</option>`).join('')}</select>
        <button class="small" id="whNew">+ Nuovo</button>${w ? '<button class="small danger" id="whDel">Elimina</button>' : ''}</h3>
        <p class="hint">Geometria, corsie/trasli, strategia di stoccaggio (ABC, distribuzione articoli, sagome) e riorganizzazione automatica. Gestione operativa nel runtime: <b>WCS</b>.</p>
        ${w ? '<div id="whForm"></div>' : '<p class="muted">Nessun magazzino.</p>'}</div>
      ${w ? '<div class="sec"><h3>Corsie e trasloelevatori</h3><p class="hint">Il traslo è un\'istanza UDT con il telegramma missione (Mission, Type, Src/Dst, Start, Busy, Done, Fault; suffisso 2 per il secondo posto UdC).</p><div id="whAisles"></div></div><div id="whStrat"></div>' : ''}`;
    main.querySelector('#whSel').onchange = (e) => { whSel = +e.target.value; drawWh(); };
    main.querySelector('#whNew').onclick = () => {
      list.push({ name: `MAG${list.length + 1}`, aisles: [{ name: '1', crane: '', slots: 1, pair_wait_s: 4 }], sides: 2, columns: 20, levels: 5, inputs: [], outputs: ['OUT1'], strategy: {}, reorg: {} });
      whSel = list.length - 1; markDirty(); drawWh();
    };
    if (!w) return;
    main.querySelector('#whDel').onclick = () => { if (confirm(`Eliminare il magazzino ${w.name}? Le locazioni salvate restano nel database.`)) { list.splice(whSel, 1); whSel = 0; markDirty(); drawWh(); } };
    w.strategy = w.strategy || {};
    w.reorg = w.reorg || {};
    const F = (obj, key, label, type = 'text', def = '', hint = '') => {
      const v = obj[key] ?? def;
      const inp = type === 'bool' ? `<input type="checkbox" data-o="${obj === w ? 'w' : obj === w.strategy ? 's' : 'r'}" data-k="${key}" data-t="bool" ${v ? 'checked' : ''}>`
        : `<input data-o="${obj === w ? 'w' : obj === w.strategy ? 's' : 'r'}" data-k="${key}" data-t="${type}" value="${esc(Array.isArray(v) ? v.join(', ') : v)}" ${type === 'number' ? 'type="number" step="any"' : ''} style="max-width:${type === 'number' ? 120 : 420}px">`;
      return `<label>${label}</label><div class="row">${inp}<span class="hint" style="margin:0">${hint}</span></div>`;
    };
    main.querySelector('#whForm').innerHTML = `<div class="formgrid">
      ${F(w, 'name', 'Nome')}${F(w, 'description', 'Descrizione')}<label>Area</label><div><select data-o="w" data-k="area" data-t="text"><option value=""></option>${areaNames().map((a) => `<option ${a === w.area ? 'selected' : ''}>${esc(a)}</option>`).join('')}</select></div>
      ${F(w, 'sides', 'Lati per corsia', 'number', 2)}${F(w, 'columns', 'Colonne', 'number', 20)}${F(w, 'levels', 'Livelli', 'number', 5)}
      ${F(w, 'block_cols', 'Colonne per blocco', 'number', 5, 'aggregazione della mappa')}
      ${F(w, 'col_time_s', 'Tempo per colonna (s)', 'number', 0.6)}${F(w, 'lvl_time_s', 'Tempo per livello (s)', 'number', 1.2)}
      ${F(w, 'level_max_height_mm', 'Altezza max per livello (mm)', 'list', [], 'dal livello 1, separati da virgola; vuoto = nessun limite')}
      ${F(w, 'inputs', 'Ingressi', 'list', [])}${F(w, 'outputs', 'Baie d\'uscita', 'list', [])}
      ${F(w, 'lu_prefix', 'Prefisso UdC demo', 'text', 'LU')}${F(w, 'initial_fill_pct', 'Riempimento iniziale %', 'number', 0, 'solo alla creazione')}${F(w, 'demo_skus', 'Articoli demo', 'number', 0)}
      ${F(w, 'auto_outbound_s', 'Prelievo casuale ogni (s)', 'number', 0, 'demo, 0 = no')}${F(w, 'auto_orders_s', 'Ordine casuale ogni (s)', 'number', 0, 'demo, 0 = no')}</div>`;
    main.querySelector('#whStrat').innerHTML = `<div class="sec"><h3>Strategia di stoccaggio</h3><div class="formgrid" id="fS">
      ${F(w.strategy, 'abc', 'Classi ABC', 'bool', true, 'zone per tempo di traslazione dal P&D')}
      ${F(w.strategy, 'a_zone_pct', 'Zona A (quota celle)', 'number', 0.2)}${F(w.strategy, 'b_zone_pct', 'Zona B (quota celle)', 'number', 0.3, 'il resto è zona C')}
      ${F(w.strategy, 'a_sku_pct', 'Classe A (quota articoli)', 'number', 0.2, 'articoli più prelevati')}${F(w.strategy, 'b_sku_pct', 'Classe B (quota articoli)', 'number', 0.3)}
      ${F(w.strategy, 'abc_window_h', 'Finestra frequenze (h)', 'number', 168)}${F(w.strategy, 'spread_sku', 'Distribuisci articolo su più corsie', 'bool', true)}</div></div>
      <div class="sec"><h3>Riorganizzazione automatica (defrag)</h3><div class="formgrid">
      ${F(w.reorg, 'enabled', 'Attiva', 'bool', false, 'stato iniziale; commutabile dal runtime')}
      ${F(w.reorg, 'idle_s', 'Inattività corsia (s)', 'number', 15, 'prima di una movimentazione')}${F(w.reorg, 'max_per_hour', 'Mosse max per ora/corsia', 'number', 60)}
      ${F(w.reorg, 'min_gain_s', 'Guadagno minimo (s)', 'number', 2, 'per la compattazione')}</div></div>`;
    main.querySelectorAll('[data-o]').forEach((el) => (el.onchange = () => {
      const o = el.dataset.o === 'w' ? w : el.dataset.o === 's' ? w.strategy : w.reorg;
      const t = el.dataset.t, k = el.dataset.k;
      let v = t === 'bool' ? el.checked : t === 'number' ? (el.value === '' ? undefined : +el.value) : t === 'list' ? el.value.split(',').map((x) => x.trim()).filter(Boolean) : el.value;
      if (k === 'level_max_height_mm') v = v.map(Number).filter((x) => x > 0);
      if (v === undefined || (t === 'text' && v === '' && k !== 'name')) delete o[k]; else o[k] = v;
      markDirty();
      if (k === 'name') drawWh();
    }));
    tableEditor(main.querySelector('#whAisles'), w.aisles, [
      { key: 'name', label: 'Corsia', width: 80, required: true },
      { key: 'crane', label: 'Traslo (istanza UDT)', width: 160, list: 'instNamesP', required: true },
      { key: 'slots', label: 'Posti UdC', type: 'select', options: [{ value: 1, label: '1' }, { value: 2, label: '2 (doppia)' }], empty: false },
      { key: 'pair_wait_s', label: 'Attesa compagna (s)', type: 'number', width: 90 },
    ], { onChange: () => { w.aisles.forEach((a) => (a.slots = +a.slots || 1)); markDirty(); }, newRow: () => ({ name: String(w.aisles.length + 1), crane: '', slots: 1, pair_wait_s: 4 }) });
    if (!document.getElementById('instNamesP')) document.body.insertAdjacentHTML('beforeend', '<datalist id="instNamesP"></datalist>');
    document.getElementById('instNamesP').innerHTML = (P.data.udt_instances || []).map((i) => `<option value="${esc(i.name)}">${esc(i.type)}</option>`).join('');
  }

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
