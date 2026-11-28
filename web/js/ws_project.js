// Workspace "Progetto": aree (gerarchia), scan class, script a timer, driver.
import { api, esc } from './core.js';
import { P, markDirty, areaNames } from './project.js';
import { tableEditor, modal } from './forms.js';
import { can, AUTH } from './auth.js';
import { toast, fmtTs } from './core.js';
import { helpBtn } from './markdown.js';

export function mount(root, { live }) {
  let tab = 'areas';
  root.innerHTML = `<div class="wsl"><div class="side" id="pjSide"></div><div class="main" id="pjMain"></div></div>`;
  const side = root.querySelector('#pjSide'), main = root.querySelector('#pjMain');

  function drawSide() {
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">${[['areas', '🗺 Aree'], ['scan', '⏱ Scan class'], ['timers', '⏲ Script a timer'], ['wh', '🏬 Magazzini'], ['security', '🔐 Sicurezza'], ['drivers', '🔌 Driver PLC']].filter(([k]) => k !== 'wh' || window.PTWCS_MODULES?.wcs !== false)
      .map(([k, l]) => `<button data-st="${k}" class="${tab === k ? 'on' : ''}" style="text-align:left">${l}</button>`).join('')}</div>
      <p class="hint">Le modifiche sono applicate a caldo con <b>Salva progetto</b>. I driver sono definiti nel file di impianto (<code>plant.yaml</code>).</p>`;
    side.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { tab = b.dataset.st; drawSide(); draw(); }));
  }

  function draw() {
    if (tab === 'areas') {
      main.innerHTML = `<div class="sec"><h3>Aree ${helpBtn('designer-progetto#aree')}</h3><p class="hint">Ogni tag (o istanza UDT, punto di decisione, driver) appartiene a un'area: gli allarmi del tag, gli errori di qualità e la perdita di comunicazione finiscono nell'area di appartenenza. Le aree possono essere annidate (l'area padre comprende le figlie).</p><div id="arTbl"></div></div>
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
      main.innerHTML = `<div class="sec"><h3>Scan class ${helpBtn('designer-progetto#scan-class')}</h3><p class="hint">Frequenze di lettura assegnabili a tag, membri UDT e istanze. I tag senza scan class usano <code>poll_ms</code> del driver. Nello stesso ciclo i gruppi in scadenza sono letti insieme (letture a blocchi ottimizzate).</p><div id="scTbl"></div></div>`;
      tableEditor(main.querySelector('#scTbl'), P.data.scan_classes, [
        { key: 'name', label: 'Nome', width: 140, required: true }, { key: 'rate_ms', label: 'Periodo (ms)', type: 'number', width: 100, required: true },
        { key: 'description', label: 'Descrizione', width: 300 },
      ], { onChange: markDirty, newRow: () => ({ name: `scan${P.data.scan_classes.length + 1}`, rate_ms: 1000 }) });
    } else if (tab === 'timers') {
      main.innerHTML = `<div class="sec"><h3>Script a timer ${helpBtn('designer-progetto#script-a-timer')}</h3><p class="hint">Funzioni Python <code>def f(payload, ctx)</code> eseguite periodicamente nel processo HMI (separato dalle decisioni in tempo reale). Utili per logiche di supporto, calcoli KPI, simulazioni (<code>sim_magazzino</code> anima la demo).</p><div id="tmTbl"></div></div>`;
      tableEditor(main.querySelector('#tmTbl'), P.data.timer_scripts, [
        { key: 'name', label: 'Nome', width: 160, required: true },
        { key: 'function', label: 'Funzione', type: 'select', options: () => P.functions, empty: false },
        { key: 'interval_ms', label: 'Intervallo (ms)', type: 'number', width: 100, required: true },
        { key: 'enabled', label: 'Attivo', type: 'bool' },
      ], { onChange: markDirty, newRow: () => ({ name: `timer${P.data.timer_scripts.length + 1}`, function: P.functions[0] || '', interval_ms: 1000, enabled: true }) });
    } else if (tab === 'wh') {
      drawWh();
    } else if (tab === 'security') {
      drawSecurity();
    } else {
      main.innerHTML = `<div class="sec"><h3>Driver PLC ${helpBtn('designer-progetto#driver-plc')}</h3><p class="hint">Definiti in <code>plant.yaml</code> (sola lettura qui). Stato aggiornato in tempo reale.</p><div id="drv"></div></div>`;
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
    main.innerHTML = `<div class="sec"><h3>Magazzini automatici ${helpBtn('designer-progetto#geometria-e-generali')}
        <select id="whSel" style="margin-left:8px">${list.map((x, i) => `<option value="${i}" ${i === whSel ? 'selected' : ''}>${esc(x.name)}</option>`).join('')}</select>
        <button class="small" id="whNew">+ Nuovo</button>${w ? '<button class="small danger" id="whDel">Elimina</button>' : ''}</h3>
        <p class="hint">Geometria, corsie/trasli, strategia di stoccaggio (ABC, distribuzione articoli, sagome) e riorganizzazione automatica. Gestione operativa nel runtime: <b>WCS</b>.</p>
        ${w ? '<div id="whForm"></div>' : '<p class="muted">Nessun magazzino.</p>'}</div>
      ${w ? '<div class="sec"><h3>Corsie e trasloelevatori ' + helpBtn('designer-progetto#corsie-e-trasloelevatori') + '</h3><p class="hint">Il traslo è un\'istanza UDT con il telegramma missione (Mission, Type, Src/Dst, Start, Busy, Done, Fault; suffisso 2 per il secondo posto UdC).</p><div id="whAisles"></div></div><div id="whStrat"></div>' : ''}`;
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
    main.querySelector('#whStrat').innerHTML = `<div class="sec"><h3>Strategia di stoccaggio ${helpBtn('designer-progetto#strategia-di-stoccaggio')}</h3><div class="formgrid" id="fS">
      ${F(w.strategy, 'abc', 'Classi ABC', 'bool', true, 'zone per tempo di traslazione dal P&D')}
      ${F(w.strategy, 'a_zone_pct', 'Zona A (quota celle)', 'number', 0.2)}${F(w.strategy, 'b_zone_pct', 'Zona B (quota celle)', 'number', 0.3, 'il resto è zona C')}
      ${F(w.strategy, 'a_sku_pct', 'Classe A (quota articoli)', 'number', 0.2, 'articoli più prelevati')}${F(w.strategy, 'b_sku_pct', 'Classe B (quota articoli)', 'number', 0.3)}
      ${F(w.strategy, 'abc_window_h', 'Finestra frequenze (h)', 'number', 168)}${F(w.strategy, 'spread_sku', 'Distribuisci articolo su più corsie', 'bool', true)}</div></div>
      <div class="sec"><h3>Riorganizzazione automatica (defrag) ${helpBtn('designer-progetto#riorganizzazione-automatica')}</h3><div class="formgrid">
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

  // ------------------------------------------------------------ sicurezza
  async function drawSecurity() {
    const sec = (P.data.security ||= { enabled: false, session_minutes: 720, idle_logout_minutes: 0, roles: [] });
    sec.roles ||= [];
    const admin = can('users.manage');
    let meta = { permissions: [], sessions: [] }, users = [];
    if (admin) {
      try { [meta, users] = await Promise.all([api.get('/api/security'), api.get('/api/users')]); } catch (e) { toast(e.message, 'error'); }
    }
    const perms = meta.permissions;
    const groups = [...new Set(perms.map((p) => p.group))];
    main.innerHTML = `<div class="sec"><h3>Impostazioni ${helpBtn('designer-progetto#sicurezza')}</h3>
        ${admin ? '' : '<p class="err">Serve il permesso users.manage per modificare la sicurezza.</p>'}
        <div class="formgrid">
          <label>Login e permessi attivi</label><label class="row"><input type="checkbox" id="sEn" ${sec.enabled ? 'checked' : ''}> ${sec.enabled ? '<span class="chip ok">attiva</span>' : '<span class="chip">disattiva: tutti possono tutto</span>'}</label>
          <label>Ruolo senza login</label><select id="sAnon"><option value="">(nessuno: login obbligatorio)</option>${sec.roles.map((r) => `<option ${r.name === sec.anonymous_role ? 'selected' : ''}>${esc(r.name)}</option>`).join('')}</select>
          <label>Durata sessione (min)</label><input type="number" id="sSess" value="${sec.session_minutes ?? 720}" style="width:110px">
          <label>Logout per inattività (min)</label><div class="row"><input type="number" id="sIdle" value="${sec.idle_logout_minutes ?? 0}" style="width:110px"><span class="hint" style="margin:0">0 = mai</span></div>
        </div>
        <p class="hint">Le modifiche a impostazioni e ruoli si applicano con <b>Salva progetto</b>. Gli utenti invece sono salvati subito nel database del server (le password non finiscono mai nel progetto).
        Al primo avvio esiste l'utente <code>admin</code> con password <code>admin</code> (cambio obbligatorio al primo accesso) o quella della variabile <code>PTWCS_ADMIN_PASSWORD</code>.</p></div>
      <div class="sec"><h3>Ruoli e permessi ${helpBtn('designer-progetto#ruoli-e-permessi')}<span class="grow"></span><button class="small" id="rAdd" ${admin ? '' : 'disabled'}>+ Ruolo</button></h3>
        <div class="tbl-wrap"><table class="permtbl"><thead><tr><th>Permesso</th>${sec.roles.map((r, i) => `<th class="rc"><input data-rn="${i}" value="${esc(r.name)}" style="width:110px" ${admin ? '' : 'disabled'}>
          <div><button class="small danger" data-rdel="${i}" title="Elimina ruolo" ${admin ? '' : 'disabled'}>✕</button></div></th>`).join('')}</tr>
          <tr><td class="muted">Descrizione</td>${sec.roles.map((r, i) => `<td class="rc"><input data-rd="${i}" value="${esc(r.description || '')}" style="width:110px" ${admin ? '' : 'disabled'}></td>`).join('')}</tr>
          <tr><td><b>* tutti i permessi</b></td>${sec.roles.map((r, i) => `<td class="rc"><input type="checkbox" data-rp="${i}" data-perm-id="*" ${r.permissions.includes('*') ? 'checked' : ''} ${admin ? '' : 'disabled'}></td>`).join('')}</tr></thead>
          <tbody>${groups.map((g) => `<tr class="pg"><td colspan="${sec.roles.length + 1}">${esc(g)}</td></tr>` + perms.filter((p) => p.group === g).map((p) => `<tr><td><code>${esc(p.id)}</code><div class="muted" style="font-size:12px">${esc(p.description)}</div></td>
            ${sec.roles.map((r, i) => { const all = r.permissions.includes('*'); return `<td class="rc"><input type="checkbox" data-rp="${i}" data-perm-id="${esc(p.id)}" ${all || r.permissions.includes(p.id) ? 'checked' : ''} ${all || !admin ? 'disabled' : ''}></td>`; }).join('')}</tr>`).join('')).join('')}</tbody></table></div></div>
      <div class="sec"><h3>Utenti ${helpBtn('designer-progetto#utenti')}<span class="grow"></span><button class="small primary" id="uAdd" ${admin ? '' : 'disabled'}>+ Nuovo utente</button></h3>
        ${admin ? `<table><thead><tr><th>Utente</th><th>Nome</th><th>Ruoli</th><th>Stato</th><th>Ultimo accesso</th><th></th></tr></thead><tbody>${users.map((u) => `<tr>
          <td class="mono">${esc(u.username)}${u.username === AUTH.me?.user ? ' <span class="chip ok">tu</span>' : ''}</td><td>${esc(u.full_name || '')}</td>
          <td>${u.roles.map((r) => `<span class="chip">${esc(r)}</span>`).join('') || '<span class="muted">—</span>'}</td>
          <td>${u.enabled ? '<span class="chip ok">attivo</span>' : '<span class="chip err">disattivato</span>'}${u.must_change ? ' <span class="chip">cambio password</span>' : ''}</td>
          <td>${u.last_login ? fmtTs(u.last_login) : '<span class="muted">mai</span>'}</td>
          <td class="nowrap"><button class="small" data-ued="${esc(u.username)}">Modifica</button><button class="small" data-upw="${esc(u.username)}">Password</button><button class="small danger" data-udel="${esc(u.username)}">Elimina</button></td></tr>`).join('')}</tbody></table>
        <h4 style="margin-top:14px">Sessioni attive (${meta.sessions.length})</h4><div class="hint">${meta.sessions.map((x) => `${esc(x.username)} · dal ${fmtTs(x.created)} · ultima attività ${fmtTs(x.last_seen)}`).join('<br>') || 'nessuna'}</div>`
        : '<p class="muted">Elenco utenti visibile solo con il permesso users.manage.</p>'}</div>`;
    if (!admin) return;
    const roleList = () => sec.roles.map((r) => r.name);
    main.querySelector('#sEn').onchange = (e) => {
      if (e.target.checked && !confirm('Attivare login e permessi?\nDopo "Salva progetto" serve il login: verifica di conoscere la password di un utente amministratore (al primo avvio admin/admin).')) { e.target.checked = false; return; }
      sec.enabled = e.target.checked; markDirty(); drawSecurity();
    };
    main.querySelector('#sAnon').onchange = (e) => { if (e.target.value) sec.anonymous_role = e.target.value; else delete sec.anonymous_role; markDirty(); };
    main.querySelector('#sSess').onchange = (e) => { sec.session_minutes = Math.max(5, +e.target.value || 720); markDirty(); };
    main.querySelector('#sIdle').onchange = (e) => { sec.idle_logout_minutes = Math.max(0, +e.target.value || 0); markDirty(); };
    main.querySelector('#rAdd').onclick = () => { sec.roles.push({ name: `ruolo${sec.roles.length + 1}`, description: '', permissions: ['runtime.view'] }); markDirty(); drawSecurity(); };
    main.querySelectorAll('[data-rn]').forEach((el) => (el.onchange = () => {
      const r = sec.roles[+el.dataset.rn], old = r.name, n = el.value.trim();
      if (!n || roleList().filter((x) => x === n).length) { el.value = old; return toast('Nome ruolo vuoto o duplicato', 'error'); }
      r.name = n;
      if (sec.anonymous_role === old) sec.anonymous_role = n;
      markDirty(); toast(`Ruolo rinominato: aggiorna gli utenti che avevano "${old}"`);
    }));
    main.querySelectorAll('[data-rd]').forEach((el) => (el.onchange = () => { sec.roles[+el.dataset.rd].description = el.value; markDirty(); }));
    main.querySelectorAll('[data-rdel]').forEach((b) => (b.onclick = () => { const r = sec.roles[+b.dataset.rdel]; if (confirm(`Eliminare il ruolo ${r.name}?`)) { sec.roles.splice(+b.dataset.rdel, 1); markDirty(); drawSecurity(); } }));
    main.querySelectorAll('[data-rp]').forEach((el) => (el.onchange = () => {
      const r = sec.roles[+el.dataset.rp], id = el.dataset.permId;
      if (id === '*') r.permissions = el.checked ? ['*'] : [];
      else r.permissions = el.checked ? [...new Set([...r.permissions, id])] : r.permissions.filter((x) => x !== id);
      markDirty(); if (id === '*') drawSecurity();
    }));
    const userDialog = (u) => {
      const isNew = !u;
      u ||= { username: '', full_name: '', roles: [], enabled: true };
      const back = modal(isNew ? 'Nuovo utente' : `Utente ${u.username}`, `
        <div class="field"><label>Utente</label><input id="uN" value="${esc(u.username)}" ${isNew ? '' : 'disabled'} autocomplete="off"></div>
        <div class="field"><label>Nome completo</label><input id="uF" value="${esc(u.full_name || '')}"></div>
        <div class="field"><label>Ruoli</label><div>${roleList().map((r) => `<label class="chip" style="cursor:pointer"><input type="checkbox" data-ur="${esc(r)}" ${u.roles.includes(r) ? 'checked' : ''}> ${esc(r)}</label>`).join('')}</div></div>
        <div class="field"><label>Attivo</label><input type="checkbox" id="uE" ${u.enabled ? 'checked' : ''}></div>
        ${isNew ? `<div class="field"><label>Password</label><input id="uP" type="password" autocomplete="new-password" placeholder="min. 6 caratteri"></div>
        <div class="field"><label>Cambio al primo accesso</label><input type="checkbox" id="uM" checked></div>` : ''}
        <p class="hint">I ruoli devono esistere nel progetto salvato.</p>`, [
        ['Annulla', '', null],
        ['Salva', 'primary', async (b) => {
          const name = b.querySelector('#uN').value.trim();
          const body = { full_name: b.querySelector('#uF').value.trim(), roles: [...b.querySelectorAll('[data-ur]')].filter((x) => x.checked).map((x) => x.dataset.ur), enabled: b.querySelector('#uE').checked };
          if (isNew) { body.password = b.querySelector('#uP').value; body.must_change = b.querySelector('#uM').checked; }
          try { await api.put(`/api/users/${encodeURIComponent(name)}`, body); toast(`Utente ${name} salvato`, 'ok'); drawSecurity(); } catch (e) { toast(e.message, 'error'); return false; }
        }]]);
      back.querySelector(isNew ? '#uN' : '#uF').focus();
    };
    main.querySelector('#uAdd').onclick = () => userDialog(null);
    main.querySelectorAll('[data-ued]').forEach((b) => (b.onclick = () => userDialog(users.find((u) => u.username === b.dataset.ued))));
    main.querySelectorAll('[data-upw]').forEach((b) => (b.onclick = () => {
      const u = users.find((x) => x.username === b.dataset.upw);
      modal(`Password di ${u.username}`, `<div class="field"><label>Nuova password</label><input id="pw1" type="password" autocomplete="new-password"></div>
        <div class="field"><label>Cambio al prossimo accesso</label><input type="checkbox" id="pwM" checked></div>
        <p class="hint">Le sessioni aperte dell'utente vengono chiuse.</p>`, [['Annulla', '', null], ['Imposta', 'primary', async (bk) => {
        try { await api.put(`/api/users/${encodeURIComponent(u.username)}`, { full_name: u.full_name, roles: u.roles, enabled: u.enabled, password: bk.querySelector('#pw1').value, must_change: bk.querySelector('#pwM').checked }); toast('Password impostata', 'ok'); drawSecurity(); } catch (e) { toast(e.message, 'error'); return false; }
      }]]);
    }));
    main.querySelectorAll('[data-udel]').forEach((b) => (b.onclick = async () => {
      if (!confirm(`Eliminare l'utente ${b.dataset.udel}?`)) return;
      try { await api.del(`/api/users/${encodeURIComponent(b.dataset.udel)}`); toast('Utente eliminato', 'ok'); drawSecurity(); } catch (e) { toast(e.message, 'error'); }
    }));
  }

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
