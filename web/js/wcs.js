// Runtime WCS: gestione missioni, ordini, giacenze, riorganizzazione, KPI trasli e strategia
// dei magazzini automatici a trasloelevatori.
import { api, esc, fmtTs, toast } from './core.js';
import { modal } from './forms.js';
import { whCellDialog, whInvalidate } from './render.js';
import { helpBtn, slug } from './markdown.js';
import { can } from './auth.js';

const TABS = [['missions', '📋 Missioni'], ['orders', '🧾 Ordini'], ['stock', '📦 Giacenze'], ['reorg', '♻ Riorganizzazione'], ['cranes', '🏗 Trasli / KPI'], ['strategy', '🎯 Strategia']];
const KIND = { in: ['⬇', 'Deposito'], out: ['⬆', 'Prelievo'], move: ['⇄', 'Movimentazione'] };
const STATE = { queued: 'in coda', sent: 'inviata', running: 'in corso', done: 'eseguita', cancelled: 'annullata' };
const OSTATE = { waiting: ['in attesa giacenza', 'warn'], released: ['in prelievo', 'info'], done: ['evaso', 'ok'], partial: ['chiuso con mancanze', 'warn'], cancelled: ['annullato', 'err'] };
const CLS_COLOR = { A: '#22c55e', B: '#3b82f6', C: '#a855f7' };
const ZONE_BG = ['#14532d55', '#1e3a8a55', '#4c1d9555'];

const age = (ts) => {
  const s = Math.max(0, (Date.now() - ts) / 1000);
  return s < 60 ? `${s.toFixed(0)} s` : s < 3600 ? `${(s / 60).toFixed(0)} min` : s < 86400 ? `${(s / 3600).toFixed(1)} h` : `${(s / 86400).toFixed(0)} gg`;
};
const pct = (x) => `${(x * 100).toFixed(0)}%`;
const cls = (c) => (c ? `<span class="wcs-cls" style="background:${CLS_COLOR[c]}">${c}</span>` : '<span class="muted">–</span>');
const bar = (f, color = 'var(--accent)') => `<span class="wcs-bar"><span style="width:${Math.min(100, f * 100).toFixed(1)}%;background:${color}"></span></span>`;

async function act(fn, ok) {
  try {
    const r = await fn();
    if (ok) toast(typeof ok === 'function' ? ok(r) : ok, 'ok');
    whInvalidate();
    return r;
  } catch (e) {
    toast(e.message, 'error');
    return null;
  }
}

/** Pagina WCS. `setTimer(fn, ms)` registra l'aggiornamento periodico (azzerato al cambio pagina). */
export async function showWcs(page, whName, params, setTimer) {
  const list = await api.get('/api/wh');
  if (!list.length) {
    page.innerHTML = '<div class="card">Nessun magazzino automatico configurato (sezione <code>warehouses</code> del progetto).</div>';
    return;
  }
  const wh = list.find((w) => w.name === whName)?.name || list[0].name;
  const tab = TABS.some(([k]) => k === params.tab) ? params.tab : 'missions';
  const W = encodeURIComponent(wh);
  const go = (t, extra = {}) => (location.hash = `#/wcs/${W}?${new URLSearchParams({ tab: t, ...extra })}`);
  page.innerHTML = `<div class="wcs ${can('wh.operate') ? '' : 'ro'}">
    <div class="wcs-head card">
      <div class="row" style="flex-wrap:wrap;gap:12px">
        <select id="wcsWh">${list.map((w) => `<option ${w.name === wh ? 'selected' : ''}>${esc(w.name)}</option>`).join('')}</select>
        <b id="wcsDesc"></b><span class="grow"></span><div id="wcsChips" class="row" style="flex-wrap:wrap;gap:6px"></div>
      </div>
      <div class="subtabs" style="margin:10px 0 0;align-items:center">${TABS.map(([k, l]) => `<button data-t="${k}" class="${k === tab ? 'on' : ''}">${l}</button>`).join('')}
        ${helpBtn('wcs#' + slug(TABS.find(([k]) => k === tab)[1].replace(/^\S+\s/, '')), 'Guida della scheda')}</div>
    </div>
    <div id="wcsBody"></div></div>`;
  page.querySelector('#wcsWh').onchange = (e) => (location.hash = `#/wcs/${encodeURIComponent(e.target.value)}?tab=${tab}`);
  page.querySelectorAll('[data-t]').forEach((b) => (b.onclick = () => go(b.dataset.t)));
  const body = page.querySelector('#wcsBody');

  const head = async () => {
    const s = await api.get(`/api/wh/${W}/summary`);
    const el = page.querySelector('#wcsChips');
    if (!el) return;
    page.querySelector('#wcsDesc').textContent = s.description || '';
    el.innerHTML = [
      `Saturazione <b>${s.totals.fill_pct.toFixed(1)}%</b>`, `Libere <b>${s.totals.free}</b>`,
      `In coda <b>${s.missions.queued}</b>`, `In corso <b>${s.missions.running}</b>`,
      s.missions.held ? `<span style="color:var(--warn)">Sospese <b>${s.missions.held}</b></span>` : '',
      `Ordini aperti <b>${s.orders.open}</b>${s.orders.waiting ? ` <span style="color:var(--warn)">(${s.orders.waiting} in attesa)</span>` : ''}`,
      `Ultima ora: ⬇${s.missions.in_hour} ⬆${s.missions.out_hour} ⇄${s.missions.move_hour}`,
      `Riorg. ${s.reorg.enabled ? '<span style="color:var(--ok)">auto</span>' : '<span class="muted">off</span>'}`,
    ].filter(Boolean).map((c) => `<span class="chip">${c}</span>`).join('');
    return s;
  };
  await head();
  const views = { missions, orders, stock, reorg, cranes, strategy };
  await views[tab]();
  setTimer(head, 3000);

  // -------------------------------------------------------- missioni
  async function missions() {
    const f = { kind: params.kind || '', aisle: params.aisle || '', q: params.q || '' };
    body.innerHTML = `<div class="card"><div class="row" style="flex-wrap:wrap">
        <select id="fKind"><option value="">Tutti i tipi</option>${Object.entries(KIND).map(([k, [i, l]]) => `<option value="${k}" ${f.kind === k ? 'selected' : ''}>${i} ${l}</option>`).join('')}</select>
        <select id="fAisle"><option value="">Tutte le corsie</option></select>
        <input id="fQ" placeholder="UdC, articolo, cella…" value="${esc(f.q)}" style="width:200px">
        <span class="grow"></span>
        <button id="bIn">⬇ Deposito manuale</button><button id="bOut">⬆ Prelievo UdC</button><button id="bMove">⇄ Movimentazione</button>
      </div>
      <h3 style="margin-top:12px">Missioni aperte <span class="muted" id="nOpen"></span></h3><div class="tbl-wrap" id="tOpen"></div>
      <h3 style="margin-top:16px">Ultime eseguite</h3><div class="tbl-wrap" id="tDone"></div></div>`;
    const s = await api.get(`/api/wh/${W}/summary`);
    body.querySelector('#fAisle').innerHTML += s.aisles.map((a, i) => `<option value="${i + 1}" ${f.aisle == i + 1 ? 'selected' : ''}>Corsia ${esc(a.name)} (${esc(a.crane)})</option>`).join('');
    const upd = () => { f.kind = body.querySelector('#fKind').value; f.aisle = body.querySelector('#fAisle').value; f.q = body.querySelector('#fQ').value.trim().toLowerCase(); draw(); };
    body.querySelector('#fKind').onchange = upd;
    body.querySelector('#fAisle').onchange = upd;
    body.querySelector('#fQ').oninput = upd;
    body.querySelector('#bIn').onclick = () => inboundDialog(wh, s, draw);
    body.querySelector('#bOut').onclick = () => outboundDialog(wh, s, draw);
    body.querySelector('#bMove').onclick = () => moveDialog(wh, null, draw);
    const match = (m) => (!f.kind || m.kind === f.kind) && (!f.aisle || m.aisle == f.aisle)
      && (!f.q || [m.lu, m.sku, m.src && code(m.src), m.dst && code(m.dst), m.bay, m.note].some((x) => String(x || '').toLowerCase().includes(f.q)));
    async function draw() {
      const r = await api.get(`/api/wh/${W}/missions?limit=40`);
      const el = body.querySelector('#tOpen');
      if (!el) return;
      const open = r.open.filter(match).sort((a, b) => rank(a) - rank(b) || b.priority - a.priority || a.id - b.id);
      body.querySelector('#nOpen').textContent = `(${open.length}${open.length !== r.open.length ? ` di ${r.open.length}` : ''})`;
      el.innerHTML = missionTable(open, true);
      body.querySelector('#tDone').innerHTML = missionTable(r.recent.filter((m) => ['done', 'cancelled'].includes(m.state)).filter(match).slice(0, 25), false);
      el.querySelectorAll('[data-ma]').forEach((b) => (b.onclick = async () => {
        const [id, a] = b.dataset.ma.split(':');
        const m = r.open.find((x) => x.id == id);
        if (a === 'done' && !confirm(`Confermare manualmente l'esecuzione della missione ${id} (${m.lu})?\nLe celle vengono aggiornate come se il traslo l'avesse completata.`)) return;
        if (a === 'requeue' && !confirm(`Rimettere in coda la missione ${id}?\nUsare solo se il traslo non la sta eseguendo (reset del PLC).`)) return;
        const body2 = a === 'up' || a === 'down' ? { priority: Math.max(1, Math.min(9, m.priority + (a === 'up' ? 1 : -1))) } : {};
        await act(() => api.post(`/api/wh/${W}/mission/${id}/${a === 'up' || a === 'down' ? 'priority' : a}`, body2));
        draw();
      }));
      el.querySelectorAll('[data-lu]').forEach((a) => (a.onclick = () => luDialog(wh, a.dataset.lu, draw)));
    }
    await draw();
    setTimer(draw, 2000);
  }

  // -------------------------------------------------------- ordini
  async function orders() {
    body.innerHTML = `<div class="card"><div class="row"><h3 class="grow" style="margin:0">Ordini di prelievo</h3><button class="primary" id="bNew">+ Nuovo ordine</button></div>
      <p class="hint">Righe coperte con UdC intere dell'articolo in ordine <b>FIFO</b> (data di ingresso), poi per vicinanza al P&D. Gli ordini senza giacenza restano <i>in attesa</i> e vengono ricoperti all'arrivo di nuove UdC.</p>
      <div class="tbl-wrap" id="tOrd"></div></div>`;
    body.querySelector('#bNew').onclick = () => orderDialog(wh, draw);
    async function draw() {
      const os = await api.get(`/api/wh/${W}/orders`);
      const el = body.querySelector('#tOrd');
      if (!el) return;
      el.innerHTML = `<table><thead><tr><th>N.</th><th>Riferimento</th><th>Stato</th><th>Prio</th><th>Baia</th><th>Righe (prelevato / richiesto)</th><th>Missioni</th><th>Creato</th><th></th></tr></thead><tbody>
        ${os.map((o) => {
          const [sl, sc] = OSTATE[o.state] || [o.state, ''];
          const closed = ['done', 'partial', 'cancelled'].includes(o.state);
          return `<tr><td>${o.id}</td><td class="mono">${esc(o.reference)}</td><td><span class="chip ${sc}">${sl}</span></td><td>${prioBadge(o.priority)}</td><td>${esc(o.bay)}</td>
            <td>${o.lines.map((l) => `<div class="row" style="gap:6px"><span class="mono" style="min-width:64px">${esc(l.sku)}</span>${bar(l.picked / l.qty, l.picked >= l.qty ? 'var(--ok)' : 'var(--accent)')}
              <span class="muted">${l.picked} / ${l.qty}${l.allocated < l.qty ? ` <span style="color:var(--warn)">manca ${+(l.qty - l.allocated).toFixed(1)}</span>` : ''}</span></div>`).join('')}</td>
            <td>${o.missions.length}</td><td title="${fmtTs(o.created)}">${age(o.created)} fa</td>
            <td class="nowrap">${closed ? '' : `<button class="small" data-oa="${o.id}:up" title="Aumenta priorità">▲</button><button class="small" data-oa="${o.id}:down" title="Riduci priorità">▼</button>
              <button class="small" data-oa="${o.id}:close" title="Chiudi con mancanze (le missioni in corso proseguono)">Chiudi</button><button class="small danger" data-oa="${o.id}:cancel" title="Annulla le missioni in coda">Annulla</button>`}</td></tr>`;
        }).join('') || '<tr><td colspan="9" class="muted">Nessun ordine</td></tr>'}</tbody></table>`;
      el.querySelectorAll('[data-oa]').forEach((b) => (b.onclick = async () => {
        const [id, a] = b.dataset.oa.split(':');
        const o = os.find((x) => x.id == id);
        if (a === 'cancel' && !confirm(`Annullare l'ordine ${o.reference}?`)) return;
        const isP = a === 'up' || a === 'down';
        await act(() => api.post(`/api/wh/${W}/order/${id}/${isP ? 'priority' : a}`, isP ? { priority: Math.max(1, Math.min(9, o.priority + (a === 'up' ? 1 : -1))) } : {}));
        draw();
      }));
    }
    await draw();
    setTimer(draw, 3000);
  }

  // -------------------------------------------------------- giacenze
  async function stock() {
    body.innerHTML = `<div class="grid2" style="grid-template-columns:minmax(380px,1.1fr) minmax(380px,1fr)">
      <div class="card"><div class="row"><h3 class="grow" style="margin:0">Articoli</h3><input id="sF" placeholder="Filtra articolo…" style="width:160px"></div><div id="sCls" class="row" style="margin:8px 0;flex-wrap:wrap"></div><div class="tbl-wrap" id="tSku" style="max-height:62vh"></div></div>
      <div class="card"><div class="row"><h3 class="grow" style="margin:0" id="luTitle">UdC</h3><input id="luQ" placeholder="Cerca UdC / lotto…" style="width:170px"></div><div class="tbl-wrap" id="tLu" style="max-height:66vh;margin-top:8px"></div></div></div>`;
    const st = await api.get(`/api/wh/${W}/stock`);
    let sel = params.sku || '';
    body.querySelector('#sCls').innerHTML = ['A', 'B', 'C'].map((c, i) => `<span class="chip">${cls(c)} ${st.classes[i]} articoli · zona ${st.zone_cells[i]} celle</span>`).join('')
      + `<span class="chip">${st.lus} UdC · ${st.picks_window} prelievi in ${st.strategy.abc_window_h} h</span>`;
    const drawSku = () => {
      const q = body.querySelector('#sF').value.trim().toLowerCase();
      body.querySelector('#tSku').innerHTML = `<table><thead><tr><th>Articolo</th><th>Cl.</th><th>Prelievi</th><th>UdC</th><th>Q.tà</th><th>Per corsia</th><th>Zone A/B/C</th><th>Più vecchia</th></tr></thead><tbody>
        ${st.skus.filter((s) => !q || s.sku.toLowerCase().includes(q)).map((s) => `<tr data-sku="${esc(s.sku)}" style="cursor:pointer" class="${s.sku === sel ? 'sel' : ''}"><td class="mono">${esc(s.sku)}</td><td>${cls(s.class)}</td><td>${s.picks}</td><td>${s.lus}</td><td>${s.qty}</td>
          <td class="mono">${s.aisles.join(' · ')}</td><td>${zoneBar(s.zones)}</td><td>${s.oldest ? age(s.oldest) : ''}</td></tr>`).join('')}</tbody></table>`;
      body.querySelectorAll('[data-sku]').forEach((r) => (r.onclick = () => { sel = r.dataset.sku; drawSku(); drawLu(); }));
    };
    const drawLu = async () => {
      const q = body.querySelector('#luQ').value.trim();
      const qs = new URLSearchParams({ limit: 300 });
      if (q) qs.set('q', q); else if (sel && sel !== '—') qs.set('sku', sel);
      const lus = await api.get(`/api/wh/${W}/lus?${qs}`);
      body.querySelector('#luTitle').textContent = q ? `UdC "${q}"` : sel ? `UdC articolo ${sel} (FIFO)` : 'UdC (FIFO)';
      body.querySelector('#tLu').innerHTML = `<table><thead><tr><th>UdC</th><th>Articolo</th><th>Q.tà</th><th>Lotto</th><th>H mm</th><th>Cella</th><th>Zona</th><th>Stato</th><th>Ingresso</th></tr></thead><tbody>
        ${lus.map((l) => `<tr><td><a href="javascript:void 0" data-lu="${esc(l.lu)}" class="mono" style="color:var(--accent)">${esc(l.lu)}</a></td><td class="mono">${esc(l.info.sku)} ${cls(l.class)}</td><td>${l.info.qty}</td>
          <td>${esc(l.info.lot)}</td><td>${l.info.height_mm || ''}</td><td class="mono"><a href="javascript:void 0" data-cell='${JSON.stringify(l.pos)}' style="color:var(--text)">${esc(l.code)}</a></td>
          <td><span class="wcs-zone z${'ABC'.indexOf(l.zone)}">${l.zone}</span>${l.class && l.class !== l.zone ? ' <span title="fuori zona" style="color:var(--warn)">⚠</span>' : ''}</td><td>${esc(l.state)}</td><td title="${fmtTs(l.info.entry)}">${age(l.info.entry)}</td></tr>`).join('') || '<tr><td colspan="9" class="muted">Nessuna UdC</td></tr>'}</tbody></table>`;
      body.querySelectorAll('[data-lu]').forEach((a) => (a.onclick = () => luDialog(wh, a.dataset.lu, drawLu)));
      body.querySelectorAll('[data-cell]').forEach((a) => (a.onclick = () => whCellDialog(wh, JSON.parse(a.dataset.cell), { api, toast })));
    };
    body.querySelector('#sF').oninput = drawSku;
    let tq;
    body.querySelector('#luQ').oninput = () => { clearTimeout(tq); tq = setTimeout(drawLu, 250); };
    drawSku();
    await drawLu();
  }

  // -------------------------------------------------------- riorganizzazione
  async function reorg() {
    const s = await api.get(`/api/wh/${W}/summary`);
    body.innerHTML = `<div class="card"><div class="row" style="flex-wrap:wrap">
        <h3 class="grow" style="margin:0">Riorganizzazione (defrag) del magazzino</h3>
        <label class="row"><input type="checkbox" id="rAuto" data-perm="wh.config"> Automatica nei tempi morti</label>
        <select id="rAisle"><option value="0">Tutte le corsie</option>${s.aisles.map((a, i) => `<option value="${i + 1}">Corsia ${esc(a.name)}</option>`).join('')}</select>
        <label class="row">Mosse max <input type="number" id="rMax" value="100" min="1" max="2000" style="width:80px"></label>
        <button id="rSim">▶ Simula piano</button><label class="row">Esegui <input type="number" id="rRun" value="10" min="1" max="500" style="width:64px"></label><button class="primary" id="rGo" data-perm="wh.config">Crea movimentazioni</button></div>
      <p class="hint" id="rCfg"></p><div id="rRes"></div></div>
      <div class="card"><div class="row"><h3 class="grow" style="margin:0">Mappa ABC</h3>
        <select id="mA">${s.aisles.map((a, i) => `<option value="${i + 1}">Corsia ${esc(a.name)}</option>`).join('')}</select>
        <select id="mS">${Array.from({ length: s.geometry.sides }, (_, i) => `<option value="${i + 1}">Lato ${i + 1}</option>`).join('')}</select></div>
        <p class="hint">Sfondo = zona della cella (vicinanza al P&D, a sinistra in basso); punto = classe dell'articolo nella cella. Celle con classe diversa dalla zona sono candidate alla riorganizzazione.</p>
        <div id="abcMap"></div></div>`;
    const plan0 = await api.get(`/api/wh/${W}/reorg?max=0`);
    const auto = body.querySelector('#rAuto');
    auto.checked = !!plan0.enabled;
    const c = plan0.config;
    body.querySelector('#rCfg').innerHTML = `Automatica: una movimentazione per corsia dopo <b>${c.idle_s} s</b> di inattività, max <b>${c.max_per_hour}</b>/h per corsia, guadagno minimo di compattazione <b>${c.min_gain_s} s</b>.
      Ordine delle mosse: 1) promozione UdC in zona peggiore della classe, 2) liberazione della zona occupata da classi inferiori, 3) compattazione verso il P&D.`;
    auto.onchange = () => act(() => api.post(`/api/wh/${W}/reorg`, { enabled: auto.checked }), auto.checked ? 'Riorganizzazione automatica attiva' : 'Riorganizzazione automatica disattivata').then(head);
    const aisleSel = () => +body.querySelector('#rAisle').value;
    body.querySelector('#rSim').onclick = simulate;
    body.querySelector('#rGo').onclick = async () => {
      const n = +body.querySelector('#rRun').value || 10;
      const r = await act(() => api.post(`/api/wh/${W}/reorg`, { run: n, aisle: aisleSel() }), (r) => `${r.created.length} movimentazioni create (priorità 1)`);
      if (r) simulate();
    };
    async function simulate() {
      const r = await api.get(`/api/wh/${W}/reorg?aisle=${aisleSel()}&max=${+body.querySelector('#rMax').value || 100}`);
      const p = r.plan;
      const row = (label, b, a, fmt, better = 'lower') => {
        const d = a - b, good = better === 'lower' ? d < 0 : d > 0;
        return `<tr><td>${label}</td><td>${fmt(b)}</td><td>${fmt(a)}</td><td style="color:${d === 0 ? 'var(--muted)' : good ? 'var(--ok)' : 'var(--alarm)'}">${d === 0 ? '=' : (d > 0 ? '+' : '') + fmt(d)}</td></tr>`;
      };
      const eff = (m) => (m.cost > 0 ? m.ideal_cost / m.cost : 1);
      body.querySelector('#rRes').innerHTML = `<div class="grid2">
        <div><h4>Indicatori (${p.moves.length} mosse simulate)</h4><table><thead><tr><th></th><th>Attuale</th><th>Dopo il piano</th><th>Δ</th></tr></thead><tbody>
          ${row('Indice di frammentazione', p.before.frag_index, p.after.frag_index, (x) => pct(x))}
          ${row('UdC fuori zona ABC', p.before.misplaced, p.after.misplaced, (x) => x)}
          ${row('Costo di accesso pesato (s)', p.before.cost, p.after.cost, (x) => x.toFixed(0))}
          ${row('Efficienza layout (ideale / attuale)', eff(p.before), eff(p.after), (x) => pct(x), 'higher')}
          </tbody></table>
          <table style="margin-top:10px"><thead><tr><th>Corsia</th><th>Mosse</th><th>Framm.</th><th>Fuori zona</th><th>Efficienza</th></tr></thead><tbody>
          ${p.aisles.map(([a, b, af, n]) => `<tr><td>${a}</td><td>${n}</td><td>${pct(b.frag_index)} → ${pct(af.frag_index)}</td><td>${b.misplaced} → ${af.misplaced}</td><td>${pct(eff(b))} → ${pct(eff(af))}</td></tr>`).join('')}</tbody></table></div>
        <div><h4>Mosse pianificate</h4><div class="tbl-wrap" style="max-height:340px"><table><thead><tr><th>#</th><th>UdC</th><th>Cl.</th><th>Da</th><th>A</th><th>Guadagno</th><th>Motivo</th></tr></thead><tbody>
          ${p.moves.slice(0, 300).map((m, i) => `<tr><td>${i + 1}</td><td class="mono">${esc(m.lu)}</td><td>${cls(m.class)}</td><td class="mono">${esc(m.from_code)}</td><td class="mono">${esc(m.to_code)}</td>
            <td>${m.gain_s >= 0 ? '−' : '+'}${Math.abs(m.gain_s).toFixed(1)} s</td><td class="muted">${esc(m.reason)}</td></tr>`).join('') || '<tr><td colspan="7" class="muted">Nessuna mossa utile: magazzino già ordinato</td></tr>'}</tbody></table></div></div></div>`;
    }
    const drawMap = async () => {
      const a = body.querySelector('#mA').value, sd = body.querySelector('#mS').value;
      const r = await api.get(`/api/wh/${W}/side?aisle=${a}&side=${sd}`);
      const el = body.querySelector('#abcMap');
      if (!el) return;
      const cw = Math.max(8, Math.min(18, Math.floor((el.clientWidth - 40) / r.columns))), ch = 14;
      let svg = '';
      for (let l = 0; l < r.levels; l++) {
        for (let c = 0; c < r.columns; c++) {
          const x = 30 + c * cw, y = (r.levels - 1 - l) * ch;
          const z = r.zones[l][c], k = r.classes[l][c], st = r.states[l][c];
          svg += `<rect x="${x}" y="${y}" width="${cw - 1}" height="${ch - 1}" fill="${st === 4 ? '#7f1d1d' : ZONE_BG[z]}" data-c="${c + 1}" data-l="${l + 1}"><title>${r.lus[l][c] || 'libera'} · zona ${'ABC'[z]}${k != null ? ` · classe ${'ABC'[k]}` : ''}</title></rect>`;
          if (st && st !== 4) svg += `<circle cx="${x + cw / 2 - 0.5}" cy="${y + ch / 2 - 0.5}" r="${Math.min(cw, ch) / 2 - 2.5}" fill="${k != null ? CLS_COLOR['ABC'[k]] : '#94a3b8'}" opacity="${st === 1 ? 1 : 0.45}" ${k != null && k !== z ? 'stroke="#fbbf24" stroke-width="1.5"' : ''} pointer-events="none"/>`;
        }
        svg += `<text x="24" y="${(r.levels - 1 - l) * ch + 10}" fill="#8b9bb8" font-size="9" text-anchor="end">${l + 1}</text>`;
      }
      svg += `<text x="30" y="${r.levels * ch + 11}" fill="#8b9bb8" font-size="9">P&D ← col. 1</text><text x="${30 + r.columns * cw}" y="${r.levels * ch + 11}" fill="#8b9bb8" font-size="9" text-anchor="end">col. ${r.columns}</text>`;
      el.innerHTML = `<svg width="${40 + r.columns * cw}" height="${r.levels * ch + 16}" style="display:block">${svg}</svg>
        <div class="row" style="gap:12px;margin-top:6px;font-size:12px">${['A', 'B', 'C'].map((c, i) => `<span class="row" style="gap:4px"><span style="width:14px;height:12px;background:${ZONE_BG[i]};border:1px solid #334155"></span>zona ${c}</span><span class="row" style="gap:4px"><span class="wcs-dot" style="background:${CLS_COLOR[c]}"></span>classe ${c}</span>`).join('')}<span class="row" style="gap:4px"><span class="wcs-dot" style="background:#64748b;outline:1.5px solid #fbbf24"></span>fuori zona</span></div>`;
      el.querySelectorAll('rect[data-c]').forEach((rc) => (rc.onclick = () => whCellDialog(wh, { aisle: +a, side: +sd, col: +rc.dataset.c, lvl: +rc.dataset.l }, { api, toast })));
    };
    body.querySelector('#mA').onchange = drawMap;
    body.querySelector('#mS').onchange = drawMap;
    await simulate();
    await drawMap();
    setTimer(drawMap, 5000);
  }

  // -------------------------------------------------------- trasli / KPI
  async function cranes() {
    body.innerHTML = `<div class="grid-areas" id="kC"></div><div class="card"><h3>Missioni eseguite – ultima ora (intervalli di 5 min)</h3><svg id="kS" viewBox="0 0 760 200" style="width:100%;max-width:900px;height:220px"></svg></div>`;
    const draw = async () => {
      const k = await api.get(`/api/wh/${W}/kpi`);
      const el = body.querySelector('#kC');
      if (!el) return;
      el.innerHTML = k.cranes.map((c) => `<div class="card wcs-crane ${c.faulted ? 'fault' : ''}"><div class="row"><h3 class="grow" style="margin:0">🏗 ${esc(c.crane)} <span class="muted">corsia ${esc(c.name)}${c.slots === 2 ? ' · doppia UdC' : ''}</span></h3>
          <span class="chip ${c.faulted ? 'err' : c.busy ? 'ok' : ''}">${c.faulted ? 'GUASTO' : c.busy ? 'in ciclo' : 'fermo'}</span></div>
        <div class="wcs-kpis">
          <div><label>Utilizzo</label>${bar(c.utilization, c.utilization > 0.85 ? 'var(--warn)' : 'var(--ok)')} <b>${pct(c.utilization)}</b></div>
          <div><label>Disponibilità</label>${bar(c.availability, c.availability < 0.95 ? 'var(--alarm)' : 'var(--ok)')} <b>${pct(c.availability)}</b></div>
          <div><label>Missioni/h</label><b class="big">${c.per_hour.toFixed(0)}</b></div>
          <div><label>Ciclo medio</label><b class="big">${c.avg_cycle_s.toFixed(0)}</b> s</div>
          <div><label>⬇ / ⬆ / ⇄</label><b>${c.in} / ${c.out} / ${c.move}</b></div>
          <div><label>Cicli${c.slots === 2 ? ' (doppi)' : ''}</label><b>${c.cycles}${c.slots === 2 ? ` (${c.double_cycles})` : ''}</b></div>
          <div><label>In coda</label><b>${c.queued}</b></div></div></div>`).join('');
      const S = k.series, max = Math.max(4, ...S.map((b) => b[0] + b[1] + b[2]));
      const col = ['#22c55e', '#3b82f6', '#a855f7'];
      let svg = `<rect width="760" height="200" fill="#0a1120"/>`;
      for (let i = 0; i <= 4; i++) svg += `<line x1="40" x2="750" y1="${10 + i * 40}" y2="${10 + i * 40}" stroke="#1e293b"/><text x="34" y="${14 + i * 40}" fill="#8b9bb8" font-size="10" text-anchor="end">${Math.round(max * (1 - i / 4))}</text>`;
      S.forEach((b, i) => {
        let y = 170;
        b.forEach((v, j) => { const h = (v / max) * 160; y -= h; svg += `<rect x="${46 + i * 59}" y="${y}" width="48" height="${h}" fill="${col[j]}"><title>${['depositi', 'prelievi', 'movimentazioni'][j]}: ${v}</title></rect>`; });
        svg += `<text x="${70 + i * 59}" y="188" fill="#8b9bb8" font-size="10" text-anchor="middle">−${(12 - i) * 5}′</text>`;
      });
      svg += ['Depositi', 'Prelievi', 'Movimentazioni'].map((l, j) => `<rect x="${560 + j * 0}" y="${14 + j * 14}" width="10" height="10" fill="${col[j]}"/><text x="${575}" y="${23 + j * 14}" fill="#cbd5e1" font-size="11">${l}</text>`).join('');
      body.querySelector('#kS').innerHTML = svg;
    };
    await draw();
    setTimer(draw, 3000);
  }

  // -------------------------------------------------------- strategia
  async function strategy() {
    const [st, s] = await Promise.all([api.get(`/api/wh/${W}/stock`), api.get(`/api/wh/${W}/summary`)]);
    const g = st.strategy;
    const proj = await api.get('/api/project');
    const def = (proj.warehouses || []).find((w) => w.name === wh) || {};
    body.innerHTML = `<div class="grid2">
      <div class="card"><h3>Strategia di stoccaggio</h3><table><tbody>
        <tr><td>Classi ABC per frequenza di prelievo</td><td><b>${g.abc ? 'attive' : 'disattive'}</b></td></tr>
        <tr><td>Finestra di calcolo</td><td>${g.abc_window_h} h (${st.picks_window} prelievi)</td></tr>
        <tr><td>Classe A / B / C (articoli)</td><td>primo ${pct(g.a_sku_pct)} / successivo ${pct(g.b_sku_pct)} / resto (e mai prelevati)</td></tr>
        <tr><td>Zona A / B / C (celle per tempo di traslazione)</td><td>${pct(g.a_zone_pct)} / ${pct(g.b_zone_pct)} / ${pct(Math.max(0, 1 - g.a_zone_pct - g.b_zone_pct))} → ${st.zone_cells.join(' / ')} celle</td></tr>
        <tr><td>Distribuzione articolo su più corsie</td><td><b>${g.spread_sku ? 'sì' : 'no'}</b></td></tr>
        <tr><td>Altezza max per livello</td><td>${(def.level_max_height_mm || []).length ? def.level_max_height_mm.map((h, i) => `L${i + 1}: ${h}`).join(' · ') + ' mm' : 'nessun limite'}</td></tr>
        <tr><td>Tempi asse X / Y</td><td>${def.col_time_s ?? '-'} s/colonna · ${def.lvl_time_s ?? '-'} s/livello</td></tr>
      </tbody></table><p class="hint">Parametri modificabili nel Designer → Progetto → Magazzini (applicazione a caldo).</p></div>
      <div class="card"><h3>Regole di assegnazione</h3><ol class="wcs-rules">
        <li><b>Corsia</b>: punteggio minimo tra le corsie con traslo disponibile e cella compatibile = missioni in coda + 0,5 × UdC dello stesso articolo già nella corsia − saturazione libera.</li>
        <li><b>Cella</b>: classe A → cella libera più vicina al P&D in zona A, poi B, poi C. Classe B → B, C, A. Classe C → C, B, A. Senza articolo → cella più vicina.</li>
        <li><b>Sagoma</b>: la UdC va solo su livelli con altezza massima ≥ altezza UdC.</li>
        <li><b>Prelievo ordini</b>: FIFO per data di ingresso, a parità la cella più vicina; UdC intere.</li>
        <li><b>Dispatcher</b>: priorità effettiva = priorità (1–9) + 1 punto ogni 2 min di attesa; missioni sospese escluse; trasli a doppia UdC: compagna dello stesso tipo e celle vicine.</li>
        <li><b>Riorganizzazione</b>: promozione ABC → liberazione zona → compattazione; solo nei tempi morti e a priorità 1.</li></ol>
        <h4>Corsie</h4><table><thead><tr><th>Corsia</th><th>Traslo</th><th>Posti UdC</th><th>Celle</th><th>Libere</th></tr></thead><tbody>
        ${s.aisles.map((a) => `<tr><td>${esc(a.name)}</td><td>${esc(a.crane)}</td><td>${a.slots}</td><td>${a.sides.reduce((n, x) => n + x.free + x.occupied + x.reserved + x.blocked, 0)}</td><td>${a.sides.reduce((n, x) => n + x.free, 0)}</td></tr>`).join('')}</tbody></table></div></div>`;
  }
}

// ------------------------------------------------------------ helper tabelle e dialoghi
const code = (p) => (p.col === 0 ? `C${String(p.aisle).padStart(2, '0')}-P&D` : `C${String(p.aisle).padStart(2, '0')}-L${p.side}-${String(p.col).padStart(2, '0')}-${String(p.lvl).padStart(2, '0')}`);
const rank = (m) => ({ running: 0, sent: 1, queued: 2 }[m.state] ?? 3);
const prioBadge = (p) => `<span class="wcs-prio p${p >= 8 ? 'h' : p <= 2 ? 'l' : 'n'}" title="priorità ${p}">${p}</span>`;
const zoneBar = (z) => {
  const n = z[0] + z[1] + z[2] || 1;
  return `<span class="wcs-zbar">${z.map((v, i) => `<span style="width:${(v / n) * 100}%;background:${CLS_COLOR['ABC'[i]]}" title="zona ${'ABC'[i]}: ${v}"></span>`).join('')}</span>`;
};

function missionTable(ms, open) {
  if (!ms.length) return `<p class="muted">${open ? 'Nessuna missione aperta' : 'Nessuna missione'}</p>`;
  return `<table><thead><tr><th>N.</th><th>Tipo</th>${open ? '<th>Prio</th>' : ''}<th>UdC</th><th>Articolo</th><th>Corsia</th><th>Da</th><th>A</th><th>Baia / motivo</th><th>Ordine</th><th>Stato</th><th>${open ? 'Età' : 'Durata'}</th>${open ? '<th></th>' : ''}</tr></thead><tbody>
    ${ms.map((m) => {
      const [ic, kl] = KIND[m.kind] || ['?', m.kind];
      const dur = m.finished && m.started ? `${((m.finished - m.started) / 1000).toFixed(0)} s` : '';
      return `<tr class="${m.hold ? 'wcs-held' : ''}"><td>${m.id}</td><td title="${kl}">${ic} ${kl}</td>${open ? `<td>${prioBadge(m.priority)}</td>` : ''}
        <td><a href="javascript:void 0" data-lu="${esc(m.lu)}" class="mono" style="color:var(--accent)">${esc(m.lu)}</a></td><td class="mono">${esc(m.sku || '')}</td><td>${m.aisle}</td>
        <td class="mono">${code(m.src)}</td><td class="mono">${code(m.dst)}</td><td>${esc(m.kind === 'move' ? m.note || m.bay : m.bay)}</td><td>${m.order ?? ''}</td>
        <td>${m.hold ? '<span class="chip" style="border-color:var(--warn);color:var(--warn)">sospesa</span>' : `<span class="chip ${m.state === 'running' ? 'ok' : m.state === 'cancelled' ? 'err' : ''}">${STATE[m.state] || m.state}${m.slot && m.state !== 'cancelled' ? ` · posto ${m.slot}` : ''}</span>`}</td>
        <td>${open ? age(m.created) : dur}</td>
        ${open ? `<td class="nowrap">${m.state === 'queued' ? `<button class="small" data-ma="${m.id}:up" title="Aumenta priorità">▲</button><button class="small" data-ma="${m.id}:down" title="Riduci priorità">▼</button>
          <button class="small" data-ma="${m.id}:${m.hold ? 'release' : 'hold'}" title="${m.hold ? 'Rilascia' : 'Sospendi'}">${m.hold ? '▶' : '❚❚'}</button><button class="small danger" data-ma="${m.id}:cancel" title="Annulla">✕</button>`
          : `<button class="small" data-ma="${m.id}:requeue" title="Rimetti in coda (missione persa dal PLC)">↻</button><button class="small" data-ma="${m.id}:done" title="Conferma esecuzione manuale">✓</button>`}</td>` : ''}</tr>`;
    }).join('')}</tbody></table>`;
}

function inboundDialog(wh, s, done) {
  const W = encodeURIComponent(wh);
  modal('Deposito manuale', `<div class="field"><label>UdC</label><input id="dLu" placeholder="PAL…"></div>
    <div class="field"><label>Articolo</label><input id="dSku"></div><div class="field"><label>Quantità</label><input id="dQty" type="number" value="1"></div>
    <div class="field"><label>Lotto</label><input id="dLot"></div><div class="field"><label>Altezza (mm)</label><input id="dH" type="number" value="0"></div>
    <div class="field"><label>Ingresso</label><select id="dBay"><option value="manuale">manuale</option>${s.inputs.map((i) => `<option>${esc(i)}</option>`).join('')}</select></div>
    <p class="hint">La cella è scelta dalla strategia (classe ABC dell'articolo, distribuzione sulle corsie, sagoma).</p>`, [
    ['Annulla', '', null],
    ['Deposita', 'primary', async (b) => {
      const v = (id) => b.querySelector(id).value.trim();
      const r = await act(() => api.post(`/api/wh/${W}/inbound`, { lu: v('#dLu'), sku: v('#dSku'), qty: +v('#dQty') || 0, lot: v('#dLot'), height_mm: +v('#dH') || 0, bay: v('#dBay') }), (r) => `Deposito → ${r.code}`);
      if (!r) return false;
      done?.();
    }]]);
}

function outboundDialog(wh, s, done, lu = '') {
  const W = encodeURIComponent(wh);
  modal('Prelievo UdC', `<div class="field"><label>UdC</label><input id="oLu" value="${esc(lu)}" placeholder="vuoto = UdC casuale"></div>
    <div class="field"><label>Baia d'uscita</label><select id="oBay"><option value="">(bilanciata)</option>${s.outputs.map((o) => `<option>${esc(o)}</option>`).join('')}</select></div>`, [
    ['Annulla', '', null],
    ['Preleva', 'primary', async (b) => {
      const r = await act(() => api.post(`/api/wh/${W}/outbound`, { lu: b.querySelector('#oLu').value.trim() || null, bay: b.querySelector('#oBay').value || null }), (m) => `Missione ${m.id}: ${m.lu} → ${m.bay}`);
      if (!r) return false;
      done?.();
    }]]);
}

function moveDialog(wh, lu, done) {
  const W = encodeURIComponent(wh);
  modal('Movimentazione cella → cella', `<div class="field"><label>UdC</label><input id="mLu" value="${esc(lu || '')}"></div>
    <div class="field"><label>Destinazione</label><div class="row"><input id="mS" type="number" placeholder="lato" style="width:70px"><input id="mC" type="number" placeholder="col." style="width:70px"><input id="mL" type="number" placeholder="liv." style="width:70px"></div></div>
    <p class="hint">Vuoto = cella migliore della stessa corsia secondo la strategia. Il traslo esegue una missione di tipo 3.</p>`, [
    ['Annulla', '', null],
    ['Crea missione', 'primary', async (b) => {
      const u = b.querySelector('#mLu').value.trim();
      const f = await api.get(`/api/wh/${W}/find?lu=${encodeURIComponent(u)}`).catch(() => ({ found: false }));
      if (!f.found) { toast(`UdC ${u} non trovata`, 'error'); return false; }
      const [sd, c, l] = ['#mS', '#mC', '#mL'].map((id) => +b.querySelector(id).value || 0);
      const dst = sd && c && l ? { aisle: f.pos.aisle, side: sd, col: c, lvl: l } : null;
      const r = await act(() => api.post(`/api/wh/${W}/move`, { lu: u, dst }), (m) => `Missione ${m.id}: ${code(m.src)} → ${code(m.dst)}`);
      if (!r) return false;
      done?.();
    }]]);
}

async function luDialog(wh, lu, done) {
  const W = encodeURIComponent(wh);
  let r;
  try { r = await api.get(`/api/wh/${W}/lu/${encodeURIComponent(lu)}`); } catch (e) { toast(e.message, 'error'); return; }
  const i = r.info || {};
  const s = await api.get(`/api/wh/${W}/summary`);
  const back = modal(`UdC ${lu}`, `<div class="field"><label>Cella</label><div class="mono">${esc(r.code)} <span class="muted">(${esc(r.loc.state)})</span></div></div>
    <div class="field"><label>Ingresso</label><div>${i.entry ? fmtTs(i.entry) : '—'}</div></div>
    <div class="field"><label>Articolo</label><input id="uSku" value="${esc(i.sku || '')}"></div>
    <div class="field"><label>Quantità</label><input id="uQty" type="number" step="any" value="${i.qty ?? 0}"></div>
    <div class="field"><label>Lotto</label><input id="uLot" value="${esc(i.lot || '')}"></div>
    <div class="field"><label>Altezza (mm)</label><input id="uH" type="number" value="${i.height_mm ?? 0}"></div>
    <div class="field"><label>Peso (kg)</label><input id="uW" type="number" step="any" value="${i.weight_kg ?? 0}"></div>`, [
    ['Chiudi', '', null],
    ['⇄ Sposta', '', () => { back.close(); moveDialog(wh, lu, done); }],
    ['⬆ Preleva', '', () => { back.close(); outboundDialog(wh, s, done, lu); }],
    ['Salva rettifica', 'primary', async (b) => {
      const v = (id) => b.querySelector(id).value.trim();
      const ok = await act(() => api.post(`/api/wh/${W}/lu/${encodeURIComponent(lu)}`, { sku: v('#uSku'), qty: +v('#uQty') || 0, lot: v('#uLot'), height_mm: +v('#uH') || 0, weight_kg: +v('#uW') || 0 }), 'Anagrafica UdC aggiornata');
      if (!ok) return false;
      done?.();
    }]]);
}

async function orderDialog(wh, done) {
  const W = encodeURIComponent(wh);
  const [st, s] = await Promise.all([api.get(`/api/wh/${W}/stock`), api.get(`/api/wh/${W}/summary`)]);
  const skus = st.skus.filter((x) => x.sku !== '—');
  const lines = [{ sku: skus[0]?.sku || '', qty: 10 }];
  const back = modal('Nuovo ordine di prelievo', `<datalist id="skuList">${skus.map((x) => `<option value="${esc(x.sku)}">${x.qty} pz in ${x.lus} UdC</option>`).join('')}</datalist>
    <div class="field"><label>Riferimento</label><input id="nRef" placeholder="automatico"></div>
    <div class="field"><label>Baia d'uscita</label><select id="nBay"><option value="">(bilanciata)</option>${s.outputs.map((o) => `<option>${esc(o)}</option>`).join('')}</select></div>
    <div class="field"><label>Priorità</label><input id="nPrio" type="number" min="1" max="9" value="5"></div>
    <h4>Righe</h4><div id="nLines"></div><button class="small" id="nAdd">+ Riga</button>`, [
    ['Annulla', '', null],
    ['Crea ordine', 'primary', async (b) => {
      const o = await act(() => api.post(`/api/wh/${W}/orders`, { reference: b.querySelector('#nRef').value.trim(), bay: b.querySelector('#nBay').value || null, priority: +b.querySelector('#nPrio').value || 5, lines: lines.filter((l) => l.sku && l.qty > 0) }),
        (o) => `Ordine ${o.reference}: ${o.missions.length} missioni${o.state === 'waiting' ? ' (in attesa di giacenza)' : ''}`);
      if (!o) return false;
      done?.();
    }]], { wide: false });
  const el = back.querySelector('#nLines');
  const draw = () => {
    el.innerHTML = lines.map((l, k) => {
      const av = skus.find((x) => x.sku === l.sku);
      return `<div class="row" style="margin-bottom:4px"><input list="skuList" data-k="${k}" data-f="sku" value="${esc(l.sku)}" placeholder="articolo" style="width:160px">
        <input type="number" data-k="${k}" data-f="qty" value="${l.qty}" style="width:90px"><span class="muted grow">${av ? `disp. ${av.qty} (${av.lus} UdC)` : l.sku ? '<span style="color:var(--warn)">nessuna giacenza</span>' : ''}</span>
        <button class="small" data-del="${k}">✕</button></div>`;
    }).join('');
    el.querySelectorAll('input').forEach((i) => (i.onchange = () => { lines[+i.dataset.k][i.dataset.f] = i.dataset.f === 'qty' ? +i.value : i.value.trim(); draw(); }));
    el.querySelectorAll('[data-del]').forEach((b) => (b.onclick = () => { lines.splice(+b.dataset.del, 1); draw(); }));
  };
  back.querySelector('#nAdd').onclick = () => { lines.push({ sku: '', qty: 10 }); draw(); };
  draw();
}
