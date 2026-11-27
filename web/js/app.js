// Client Runtime (operatore): viste, allarmi, decisioni, tag, trend, diagnostica.
import { api, Live, esc, fmtTs, toast } from './core.js';
import { ViewRenderer, runActions, createPopup, compileSymbol, invalidateView, whCellDialog, scopeFor, subst } from './render.js';
import { showMenu, editItems } from './ctxmenu.js';
import { initAuth, mountUserChip, can, AUTH } from './auth.js';
import { showWcs } from './wcs.js';

const live = new Live();
const page = document.getElementById('page');
let info = null, views = [], symbols = [], current = null, renderer = null, refresh = null;
let decisions = [], logs = [], compiled = {}, popups = [], areaFilter = '', routeParams = {}, timers = [], hasWh = false;
const setTimer = (fn, ms) => timers.push(setInterval(() => fn().catch?.(() => {}), ms));

// ------------------------------------------------------------ routing
function route() {
  clearInterval(refresh);
  timers.forEach(clearInterval);
  timers = [];
  popups.forEach((p) => p.win.remove());
  popups = [];
  renderer = null;
  const [hpath, hquery] = location.hash.split('?');
  const [, kind, arg] = hpath.split('/');
  routeParams = Object.fromEntries(new URLSearchParams(hquery || ''));
  page.className = 'page';
  markNav();
  if (kind === 'view' && arg) return showView(arg);
  if (kind === 'alarms') return showAlarms();
  if (kind === 'areas') return showAreas(arg);
  if (kind === 'decisions') return showDecisions();
  if (kind === 'tags') return showTags();
  if (kind === 'trend') return showTrend();
  if (kind === 'diag') return showDiag();
  if (kind === 'wcs') return showWcs(page, arg ? decodeURIComponent(arg) : '', routeParams, setTimer).catch((e) => { page.innerHTML = `<div class="card">Errore: ${esc(e.message)}</div>`; });
  const home = views.find((v) => v.home) || views[0];
  location.hash = home ? `#/view/${home.id}` : '#/tags';
}

function buildNav() {
  const n = live.alarms.filter((a) => !a.acked).length;
  document.getElementById('nav').innerHTML =
    views.filter((v) => !v.faceplate && v.menu !== false && !v.area).map((v) => `<a href="#/view/${esc(v.id)}">${esc(v.title)}</a>`).join('') +
    (hasWh ? '<a href="#/wcs">WCS</a>' : '') + `<a href="#/areas">Aree</a><a href="#/alarms">Allarmi${n ? `<span class="badge">${n}</span>` : ''}</a>
     <a href="#/decisions">Decisioni</a><a href="#/tags">Tag</a><a href="#/trend">Trend</a><a href="#/diag">Diagnostica</a>`;
  markNav();
}

function markNav() {
  document.querySelectorAll('#nav a').forEach((a) => a.classList.toggle('active', a.getAttribute('href') === location.hash.split('?')[0]
    || (location.hash.startsWith('#/areas') && a.getAttribute('href') === '#/areas')
    || (location.hash.startsWith('#/wcs') && a.getAttribute('href') === '#/wcs')));
}

// ------------------------------------------------------------ viste
async function showView(id) {
  page.className = 'page view';
  try {
    current = await api.get(`/api/views/${encodeURIComponent(id)}`);
  } catch (e) {
    page.className = 'page';
    page.innerHTML = `<div class="card">Vista <b>${esc(id)}</b> non trovata. <a href="designer.html">Creala nel Designer</a>.</div>`;
    return;
  }
  page.innerHTML = `<div class="viewhost ${AUTH.viewOps[id] === false || !can('tag.write') ? 'nocmd' : ''}"><svg xmlns="http://www.w3.org/2000/svg" preserveAspectRatio="xMidYMid meet"></svg></div>`;
  renderer = new ViewRenderer(page.querySelector('svg'), live, {
    api, symbols: compiled, onAction: actionHandler(id), params: routeParams,
    onWhCell: (wh, pos) => whCellDialog(wh, pos, { api, toast }),
  });
  renderer.load(current);
}

/** Gestore azioni per una vista (o un popup): passa lo scope del componente. */
function actionHandler(viewId) {
  return (comp, ev, scope) => runActions(comp, ev, {
    live, api, toast, viewId, scope, openPopup,
    navigate: (v, params) => {
      const q = params && Object.keys(params).length ? '?' + new URLSearchParams(params) : '';
      location.hash = `#/view/${v}${q}`;
    },
  });
}

/** Apre una vista in popup (faceplate) passandole i parametri. */
function openPopup(viewId, params = {}, title) {
  const p = createPopup({
    live, api, toast, symbols: compiled, viewId, params, title, onWhCell: (wh, pos) => whCellDialog(wh, pos, { api, toast }),
    onAction: actionHandler(viewId),
    onClose: (r) => { popups = popups.filter((x) => x.renderer !== r); },
  });
  popups.push(p);
}

// ------------------------------------------------------------ menu tasto destro: navigazione
const goView = (id) => (location.hash = `#/view/${encodeURIComponent(id)}`);

/** Viste di un'area e delle sotto-aree, come sottomenu annidati. */
function areaMenuItems(name) {
  const vs = views.filter((v) => v.area === name && !v.faceplate && v.menu !== false);
  const kids = areaData.filter((a) => a.parent === name);
  const a = areaData.find((x) => x.name === name);
  return [
    ...vs.map((v) => ({ label: v.title, icon: '🖼', checked: current && location.hash.startsWith(`#/view/${v.id}`), action: () => goView(v.id) })),
    ...kids.map((k) => ({ label: k.name, icon: sevIcon(k), sub: areaMenuItems(k.name) })),
    { sep: true },
    { label: 'Panoramica area', icon: '🗺', action: () => (location.hash = `#/areas/${encodeURIComponent(name)}`) },
    { label: `Allarmi dell'area${a?.active ? ` (${a.active})` : ''}`, icon: '🔔', action: () => { areaFilter = name; location.hash = '#/alarms'; } },
  ];
}
const sevIcon = (a) => ({ critical: '🟥', alarm: '🔴', warning: '🟠', info: '🔵' }[a.worst] || '🟢');

/** Componente sotto il mouse (vista principale o popup) con renderer e vista. */
function componentAt(e) {
  const win = e.target.closest('.popup-win');
  const r = win ? popups.find((p) => p.win === win)?.renderer : renderer;
  if (!r) return null;
  let g = e.target.closest('g[data-id]');
  while (g && g.parentNode !== r.layer) g = g.parentNode?.closest?.('g[data-id]');
  const comp = g && r.view?.components?.find((c) => c.id === g.dataset.id);
  return comp ? { comp, r } : null;
}

function runtimeMenu(e) {
  if (e.shiftKey || e.target.closest('.ctxmenu')) return; // Maiusc = menu del browser
  const hit = componentAt(e);
  const items = editItems(e, { notify: toast });
  if (hit) {
    const scope = scopeFor(hit.comp, hit.r.scope || {});
    const acts = hit.comp.events?.onClick?.actions || [];
    const pop = acts.find((a) => a.kind === 'popup' && a.view);
    const nav = acts.find((a) => a.kind === 'navigate' && a.view);
    const prm = (a) => Object.fromEntries(Object.entries(a.params || {}).map(([k, v]) => [k, subst(String(v ?? ''), scope)]));
    const dev = scope.device;
    if (pop || nav || dev) items.push({ header: dev || hit.comp.name || hit.comp.id });
    if (pop) items.push({ label: 'Apri faceplate', icon: '🪟', action: () => openPopup(pop.view, prm(pop), dev || undefined) });
    if (nav) items.push({ label: `Vai a ${views.find((v) => v.id === nav.view)?.title || nav.view}`, icon: '↪', action: () => { const q = new URLSearchParams(prm(nav)).toString(); location.hash = `#/view/${nav.view}${q ? '?' + q : ''}`; } });
    if (dev) items.push({ label: `Tag di ${dev}`, icon: '🏷', action: () => { localStorage.setItem('tagFilter', dev); location.hash = '#/tags'; } });
    if (items.length) items.push({ sep: true });
  }
  const home = views.find((v) => v.home) || views.find((v) => !v.faceplate);
  const free = views.filter((v) => !v.area && !v.faceplate && v.menu !== false);
  const roots = areaData.filter((a) => !a.parent);
  items.push(
    { label: 'Indietro', icon: '←', key: 'Alt+←', action: () => history.back() },
    { label: 'Avanti', icon: '→', key: 'Alt+→', action: () => history.forward() },
    home ? { label: `Pagina iniziale (${home.title})`, icon: '⌂', action: () => goView(home.id) } : null,
    { sep: true },
    free.length ? { header: 'Viste' } : null,
    ...free.map((v) => ({ label: v.title, icon: '🖼', checked: location.hash.split('?')[0] === `#/view/${v.id}`, action: () => goView(v.id) })),
    roots.length ? { header: 'Aree' } : null,
    ...roots.map((a) => ({ label: `${a.name}${a.active ? ` · ${a.active} all.` : ''}`, icon: sevIcon(a), sub: areaMenuItems(a.name) })),
    { sep: true },
    { label: 'Pagine di sistema', icon: '☰', sub: [
      { label: 'Panoramica aree', icon: '🗺', action: () => (location.hash = '#/areas') },
      { label: 'Allarmi', icon: '🔔', action: () => { areaFilter = ''; location.hash = '#/alarms'; } },
      hasWh ? { label: 'WCS magazzini', icon: '🏬', action: () => (location.hash = '#/wcs') } : null,
      { label: 'Decisioni', icon: '🔀', action: () => (location.hash = '#/decisions') },
      { label: 'Tag', icon: '🏷', action: () => (location.hash = '#/tags') },
      { label: 'Trend', icon: '📈', action: () => (location.hash = '#/trend') },
      { label: 'Diagnostica', icon: '🩺', action: () => (location.hash = '#/diag') }].filter(Boolean) },
    { label: 'Aggiorna pagina', icon: '⟳', key: 'F5', action: () => route() },
    { label: 'Manuale utente', icon: '📖', key: 'F1', action: () => window.open(`manual.html#${location.hash.startsWith('#/wcs') ? 'wcs' : 'runtime'}`, '_blank') },
    { label: document.fullscreenElement ? 'Esci da schermo intero' : 'Schermo intero', icon: '⛶', key: 'F11', action: () => (document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen?.()) },
    info?.designer ? { label: location.hash.startsWith('#/view/') ? 'Modifica vista nel Designer' : 'Apri il Designer', icon: '✎', action: () => {
      if (location.hash.startsWith('#/view/')) localStorage.setItem('designer.view', location.hash.split('/')[2].split('?')[0]);
      window.open('designer.html', '_blank');
    } } : null,
  );
  showMenu(e, items);
}
document.addEventListener('contextmenu', runtimeMenu);
document.addEventListener('keydown', (e) => { if (e.key === 'F1') { e.preventDefault(); window.open(`manual.html#${location.hash.startsWith('#/wcs') ? 'wcs' : 'runtime'}`, '_blank'); } });

// ------------------------------------------------------------ barra aree (stile PCS7)
let areaData = [], areaMenuFor = null;

async function refreshAreaBar() {
  try { areaData = await api.get('/api/areas'); } catch { return; }
  drawAreaBar();
}

/** Area della vista corrente (per evidenziare il pulsante). */
function currentArea() {
  const id = location.hash.startsWith('#/view/') ? location.hash.split('/')[2].split('?')[0] : null;
  return views.find((v) => v.id === id)?.area || null;
}

function drawAreaBar() {
  const bar = document.getElementById('areabar');
  if (!bar) return;
  const roots = areaData.filter((a) => !a.parent);
  const cur = currentArea();
  const inSub = (name, root) => inArea(name, root, areaData);
  bar.innerHTML = `<div class="areabtn" data-home title="Pagina iniziale"><span class="an">⌂ Impianto</span>
      <span class="ai"><span class="muted" style="background:none">${areaData.reduce((s, a) => s + (a.parent ? 0 : a.active), 0)} all.</span></span></div>` +
    roots.map((a) => {
      const sev = a.worst || '';
      const blink = a.unacked > 0 ? 'blink' : '';
      const [i, w, al, c] = a.by_sev || [0, 0, 0, 0];
      return `<div class="areabtn ${sev ? 'sev-' + sev : ''} ${blink} ${cur && inSub(cur, a.name) ? 'current' : ''} ${areaMenuFor === a.name ? 'open' : ''}" data-area="${esc(a.name)}"
          title="${esc(a.description || a.name)}\n${a.active} allarmi attivi · ${a.unacked} da riconoscere · ${a.bad_tags} tag in errore">
        <span class="an">${esc(a.name)}</span>
        <span class="ai"><span class="c ${c ? 'on' : ''}" title="critici">${c || ''}</span><span class="a ${al ? 'on' : ''}" title="allarmi">${al || ''}</span>
          <span class="w ${w ? 'on' : ''}" title="avvisi">${w || ''}</span><span class="i ${i ? 'on' : ''}" title="info">${i || ''}</span>
          <span class="q ${a.bad_tags ? 'on' : ''}" title="tag con qualità BAD">${a.bad_tags ? 'Q' : ''}</span></span></div>`;
    }).join('');
  bar.querySelector('[data-home]').onclick = () => { closeAreaMenu(); location.hash = ''; route(); };
  bar.querySelectorAll('[data-area]').forEach((b) => (b.onclick = (e) => { e.stopPropagation(); toggleAreaMenu(b.dataset.area, b); }));
  if (areaMenuFor) {
    const b = bar.querySelector(`[data-area="${CSS.escape(areaMenuFor)}"]`);
    if (b) positionMenu(b);
  }
}

function closeAreaMenu() {
  areaMenuFor = null;
  document.getElementById('areamenu')?.remove();
  document.querySelectorAll('.areabtn.open').forEach((b) => b.classList.remove('open'));
}

function positionMenu(btn) {
  const m = document.getElementById('areamenu');
  if (!m) return;
  const r = btn.getBoundingClientRect();
  m.style.left = `${Math.min(r.left, window.innerWidth - m.offsetWidth - 8)}px`;
  m.style.top = `${r.bottom + 4}px`;
}

/** Menu delle viste di un'area (e delle sotto-aree), con stato per sotto-area. */
function toggleAreaMenu(name, btn) {
  if (areaMenuFor === name) return closeAreaMenu();
  closeAreaMenu();
  areaMenuFor = name;
  btn.classList.add('open');
  const subtree = areaData.filter((a) => inArea(a.name, name, areaData));
  const groups = subtree.map((a) => ({ a, vs: views.filter((v) => v.area === a.name && !v.faceplate && v.menu !== false) }));
  const m = document.createElement('div');
  m.className = 'areamenu';
  m.id = 'areamenu';
  m.innerHTML = groups.map(({ a, vs }) => `<div class="grp"><span class="dotsev ${a.worst || ''}"></span>${esc(a.name)}
        <span style="margin-left:auto;text-transform:none;letter-spacing:0">${a.active ? `${a.active} all.` : 'OK'}</span></div>
      ${vs.map((v) => `<a href="#/view/${esc(v.id)}">🖼 ${esc(v.title)}</a>`).join('') || (a.name === name ? '' : '')}`).join('') +
    `<div class="grp">Area ${esc(name)}</div><a href="#/areas/${encodeURIComponent(name)}">🗺 Panoramica area</a>
     <a href="#/alarms" data-alarms>🔔 Allarmi dell'area</a>`;
  m.querySelector('[data-alarms]').onclick = () => { areaFilter = name; };
  m.querySelectorAll('a').forEach((x) => x.addEventListener('click', () => setTimeout(closeAreaMenu, 0)));
  document.body.appendChild(m);
  positionMenu(btn);
}
document.addEventListener('click', (e) => { if (!e.target.closest('.areamenu') && !e.target.closest('.areabtn')) closeAreaMenu(); });
window.addEventListener('resize', () => closeAreaMenu());

// ------------------------------------------------------------ aree
async function showAreas(name) {
  const draw = async () => {
    if (!location.hash.startsWith('#/areas')) return;
    const areas = await api.get('/api/areas');
    const sel = name ? decodeURIComponent(name) : '';
    const kids = (a) => areas.filter((x) => x.parent === a);
    const card = (a) => `<div class="card area-card ${a.worst ? 'sev-' + a.worst : ''}" data-area="${esc(a.name)}">
        <div class="row"><b class="grow">${esc(a.name)}</b>${a.worst ? `<span class="sev ${a.worst}">${a.worst}</span>` : '<span style="color:var(--ok)">OK</span>'}</div>
        <div class="muted">${esc(a.description || '')}</div>
        <div class="row" style="margin-top:8px;gap:18px">
          <div><div class="big">${a.active}</div><div class="muted">allarmi attivi</div></div>
          <div><div class="big">${a.unacked}</div><div class="muted">da riconoscere</div></div>
          <div><div class="big" style="color:${a.bad_tags ? 'var(--warn)' : 'inherit'}">${a.bad_tags}</div><div class="muted">tag in errore / ${a.tags}</div></div>
        </div></div>`;
    if (!sel) {
      const roots = areas.filter((a) => !a.parent);
      page.innerHTML = roots.map((r) => `<h3 style="margin:6px 0 8px">${esc(r.name)}</h3><div class="grid-areas">${card(r)}${kids(r.name).map(card).join('')}</div>`).join('');
    } else {
      const a = areas.find((x) => x.name === sel);
      const list = live.alarms.filter((al) => inArea(al.area, sel, areas));
      const vs = views.filter((v) => v.area === sel);
      page.innerHTML = `<div class="row" style="margin-bottom:10px"><a href="#/areas" style="color:var(--muted)">← Aree</a><h3 class="grow" style="margin:0 0 0 10px">${esc(sel)}</h3>
          <button id="ackArea" data-perm="alarm.ack">Riconosci allarmi area</button></div>
        <div class="grid2">${a ? card(a) : ''}${kids(sel).map(card).join('')}
          <div class="card"><h3>Viste dell'area</h3>${vs.map((v) => `<a href="#/view/${esc(v.id)}" style="color:var(--accent);display:block">${esc(v.title)}</a>`).join('') || '<span class="muted">Nessuna vista associata (proprietà "Area" della vista nel Designer)</span>'}</div></div>
        <div class="card"><h3>Allarmi</h3><table><thead><tr><th>Ora</th><th>Gravità</th><th>Area</th><th>Id</th><th>Messaggio</th><th>Stato</th><th></th></tr></thead>
        <tbody>${alarmRows(list) || '<tr><td colspan=7 class="muted">Nessun allarme</td></tr>'}</tbody></table></div>`;
      page.querySelector('#ackArea').onclick = () => api.post('/api/alarms/ack', { area: sel });
      bindAck();
    }
    page.querySelectorAll('[data-area]').forEach((c) => (c.onclick = () => (location.hash = `#/areas/${encodeURIComponent(c.dataset.area)}`)));
  };
  showAreas.draw = draw;
  await draw();
  refresh = setInterval(draw, 3000);
}

function inArea(area, target, areas) {
  let cur = area, guard = 0;
  while (cur && guard++ < 32) {
    if (cur === target) return true;
    cur = areas.find((a) => a.name === cur)?.parent;
  }
  return false;
}

function bindAck() {
  page.querySelectorAll('[data-ack]').forEach((b) => (b.onclick = () => api.post('/api/alarms/ack', { id: b.dataset.ack })));
}

// ------------------------------------------------------------ allarmi
function alarmRows(list) {
  return list.map((a) => `<tr class="${a.active ? 'active' : ''} ${a.acked ? '' : 'unacked'}">
    <td>${fmtTs(a.raised_ts)}</td><td><span class="sev ${a.severity}">${a.severity}</span></td>
    <td>${esc(a.area || '')}</td><td class="mono">${esc(a.id)}</td><td>${esc(a.message)}</td>
    <td>${a.active ? '<b style="color:var(--alarm)">ATTIVO</b>' : 'rientrato ' + fmtTs(a.cleared_ts)}</td>
    <td>${a.acked ? '✓' : `<button class="small" data-ack="${esc(a.id)}" data-perm="alarm.ack">Riconosci</button>`}</td></tr>`).join('');
}

async function showAlarms() {
  const [hist, areas] = await Promise.all([api.get('/api/alarms/history?limit=300'), api.get('/api/areas')]);
  const draw = () => {
    if (!location.hash.startsWith('#/alarms')) return;
    const f = areaFilter;
    const cur = live.alarms.filter((a) => !f || inArea(a.area, f, areas));
    const h = hist.filter((e) => !f || inArea(e.area, f, areas));
    page.innerHTML = `<div class="card"><div class="row"><h3 class="grow">Allarmi correnti</h3>
      <label class="row muted">Area <select id="areaSel"><option value="">(tutte)</option>${areas.map((a) => `<option ${a.name === f ? 'selected' : ''}>${esc(a.name)}</option>`).join('')}</select></label>
      <button id="ackAll" data-perm="alarm.ack">Riconosci ${f ? 'area' : 'tutti'}</button></div>
      <table><thead><tr><th>Ora</th><th>Gravità</th><th>Area</th><th>Id</th><th>Messaggio</th><th>Stato</th><th></th></tr></thead>
      <tbody>${alarmRows(cur) || '<tr><td colspan=7 class="muted">Nessun allarme</td></tr>'}</tbody></table></div>
      <div class="card"><h3>Storico eventi</h3><table><thead><tr><th>Ora</th><th>Evento</th><th>Area</th><th>Id</th><th>Messaggio</th><th>Valore</th><th>Utente</th></tr></thead>
      <tbody>${h.map((e) => `<tr><td>${fmtTs(e.ts)}</td><td>${esc(e.event)}</td><td>${esc(e.area || '')}</td><td class="mono">${esc(e.alarm)}</td>
        <td>${esc(e.message)}</td><td>${esc(e.value)}</td><td>${esc(e.user || '')}</td></tr>`).join('')}</tbody></table></div>`;
    page.querySelector('#ackAll').onclick = () => api.post('/api/alarms/ack', f ? { area: f } : {});
    page.querySelector('#areaSel').onchange = (e) => { areaFilter = e.target.value; draw(); };
    bindAck();
  };
  showAlarms.draw = draw;
  draw();
}

// ------------------------------------------------------------ decisioni
async function showDecisions() {
  decisions = await api.get('/api/decisions?limit=200');
  const dps = info.decision_points;
  const draw = () => {
    if (!location.hash.startsWith('#/decisions')) return;
    const okN = decisions.filter((d) => d.status === 'ok').length;
    const avg = decisions.length ? decisions.reduce((s, d) => s + d.duration_ms, 0) / decisions.length : 0;
    page.innerHTML = `<div class="grid2">${dps.map((d) => `<div class="card"><h3>${esc(d.name)} <span class="muted">– ${esc(d.description)}</span></h3>
        <div class="mono muted">${esc(d.request_tag)} → regole${d.function ? ' / ' + esc(d.function) + '()' : ''} → ${esc(d.response_tag)}</div></div>`).join('')}
      <div class="card"><h3>Ultime ${decisions.length}</h3>ok: <b>${okN}</b> · fallback/errore: <b>${decisions.length - okN}</b> · tempo medio: <b>${avg.toFixed(1)} ms</b></div></div>
      <div class="card"><table><thead><tr><th>Ora</th><th>Punto</th><th>Req</th><th>Input</th><th>Output</th><th>ms</th><th>Regola</th><th>Esito</th></tr></thead><tbody>
      ${decisions.slice(0, 200).map((d) => `<tr><td>${fmtTs(d.ts)}</td><td>${esc(d.point)}</td><td>${d.request_id}</td>
        <td class="mono">${esc(JSON.stringify(d.inputs))}</td><td class="mono">${esc(JSON.stringify(d.outputs))}</td>
        <td>${d.duration_ms.toFixed(1)}</td><td>${esc(d.rule || (d.status === 'ok' ? 'script' : ''))}</td><td>${d.status === 'ok' ? '✓' : `<span style="color:var(--warn)" title="${esc(d.error || '')}">${esc(d.status)}</span>`}</td></tr>`).join('')}
      </tbody></table></div>`;
  };
  showDecisions.draw = draw;
  draw();
}

// ------------------------------------------------------------ tag
async function showTags() {
  const defs = await api.get('/api/tags');
  page.innerHTML = `<div class="card"><div class="row"><h3 class="grow">Tag (${defs.length})</h3><input id="flt" placeholder="Filtra…"></div>
    <table><thead><tr><th>Nome</th><th>Driver</th><th>Indirizzo</th><th>Tipo</th><th>Valore</th><th>Qualità</th><th>Agg.</th><th></th></tr></thead>
    <tbody id="tb"></tbody></table></div>`;
  const tb = page.querySelector('#tb');
  tb.innerHTML = defs.map((t) => `<tr data-tag="${esc(t.name)}"><td class="mono">${esc(t.name)}<div class="muted">${esc(t.description)}</div></td>
    <td>${esc(t.driver)}</td><td class="mono">${esc(t.address)}</td><td>${esc(t.type)}</td>
    <td class="mono v"></td><td class="q"></td><td class="ts muted"></td>
    <td>${t.writable ? (t.can_write === false
      ? `<button class="small noperm" disabled title="Scrittura non consentita al tuo ruolo${t.write_roles?.length ? ' (ruoli: ' + esc(t.write_roles.join(', ')) + ')' : ''}">🔒 Scrivi</button>`
      : `<button class="small" data-w="${esc(t.name)}">Scrivi</button>`) : ''}</td></tr>`).join('');
  const upd = () => tb.querySelectorAll('tr').forEach((tr) => {
    const s = live.tags.get(tr.dataset.tag);
    if (!s) return;
    tr.querySelector('.v').textContent = JSON.stringify(s.value) + (defs.find((d) => d.name === tr.dataset.tag)?.unit ? ' ' + defs.find((d) => d.name === tr.dataset.tag).unit : '');
    tr.querySelector('.q').innerHTML = s.quality === 'good' ? '✓' : `<span class="q-bad">${s.quality}</span>`;
    tr.querySelector('.ts').textContent = new Date(s.ts).toLocaleTimeString();
  });
  showTags.draw = upd;
  upd();
  page.querySelector('#flt').oninput = (e) => {
    const f = e.target.value.toLowerCase();
    tb.querySelectorAll('tr').forEach((tr) => (tr.style.display = tr.dataset.tag.toLowerCase().includes(f) ? '' : 'none'));
  };
  const pre = localStorage.getItem('tagFilter');
  if (pre) { localStorage.removeItem('tagFilter'); const i = page.querySelector('#flt'); i.value = pre + '.'; i.dispatchEvent(new Event('input')); }
  tb.querySelectorAll('[data-w]').forEach((b) => (b.onclick = async () => {
    const v = prompt(`Nuovo valore per ${b.dataset.w}`, JSON.stringify(live.value(b.dataset.w)));
    if (v === null) return;
    let val; try { val = JSON.parse(v); } catch { val = v; }
    try { await api.post('/api/tags/write', { tag: b.dataset.w, value: val }); toast('Scritto', 'ok'); } catch (e) { toast(e.message, 'error'); }
  }));
}

// ------------------------------------------------------------ trend
async function showTrend() {
  const defs = (await api.get('/api/tags')).filter((t) => ['u8', 'i16', 'u16', 'i32', 'u32', 'f32', 'bool'].includes(t.type));
  const sel = new Set(JSON.parse(localStorage.getItem('trendTags') || '[]').filter((n) => defs.some((d) => d.name === n)));
  if (!sel.size && defs[0]) sel.add(defs.find((d) => d.name.includes('Speed'))?.name || defs[0].name);
  page.innerHTML = `<div class="card"><div class="row" style="flex-wrap:wrap">
      <select id="range"><option value="300000">5 min</option><option value="900000">15 min</option><option value="3600000" selected>1 ora</option><option value="86400000">24 ore</option></select>
      <div id="chips" class="row" style="flex-wrap:wrap"></div></div>
      <svg id="chart" viewBox="0 0 1000 380" style="width:100%;height:420px;margin-top:10px"></svg>
      <p class="muted">Solo i tag con <code>historize: true</code> nel file di configurazione hanno dati storici.</p></div>`;
  const colors = ['#3b82f6', '#22c55e', '#f59e0b', '#ef4444', '#a855f7', '#06b6d4', '#ec4899'];
  const chips = page.querySelector('#chips');
  chips.innerHTML = defs.map((d) => `<label class="row" style="gap:3px"><input type="checkbox" value="${esc(d.name)}" ${sel.has(d.name) ? 'checked' : ''}>${esc(d.name)}</label>`).join('');
  chips.onchange = (e) => { e.target.checked ? sel.add(e.target.value) : sel.delete(e.target.value); localStorage.setItem('trendTags', JSON.stringify([...sel])); draw(); };
  page.querySelector('#range').onchange = draw;
  async function draw() {
    const range = +page.querySelector('#range')?.value || 3600000;
    const to = Date.now(), from = to - range;
    const series = await Promise.all([...sel].map(async (t) => ({ t, d: (await api.get(`/api/trend?tag=${encodeURIComponent(t)}&from=${from}&to=${to}`)).filter((p) => p[1] !== null) })));
    const svg = page.querySelector('#chart');
    if (!svg) return;
    const W = 1000, H = 380, L = 60, B = 30;
    let html = `<rect width="${W}" height="${H}" fill="#0a1120"/>`;
    for (let i = 0; i <= 5; i++) {
      const x = L + (W - L - 10) * i / 5;
      html += `<line x1="${x}" y1="10" x2="${x}" y2="${H - B}" stroke="#1e293b"/><text x="${x}" y="${H - 10}" fill="#8b9bb8" font-size="11" text-anchor="middle">${new Date(from + range * i / 5).toLocaleTimeString()}</text>`;
    }
    series.forEach((s, k) => {
      if (!s.d.length) return;
      const vals = s.d.map((p) => p[1]);
      const lo = Math.min(...vals), hi = Math.max(...vals), span = hi - lo || 1;
      // ultimo valore prolungato fino a "adesso" (storico a cambiamento)
      const pts = [...s.d, [to, s.d.at(-1)[1]]];
      let path = '';
      pts.forEach(([ts, v], i) => {
        const x = L + (W - L - 10) * (ts - from) / range, y = 10 + (H - B - 20) * (1 - (v - lo) / span);
        path += i ? ` H${x.toFixed(1)} V${y.toFixed(1)}` : `M${x.toFixed(1)} ${y.toFixed(1)}`;
      });
      const c = colors[k % colors.length];
      html += `<path d="${path}" fill="none" stroke="${c}" stroke-width="1.8"/>
        <text x="${L + 6}" y="${22 + k * 16}" fill="${c}" font-size="12">${esc(s.t)}: ${lo}…${hi} (ult. ${s.d.at(-1)[1]})</text>`;
    });
    svg.innerHTML = html;
  }
  await draw();
  refresh = setInterval(draw, 5000);
}

// ------------------------------------------------------------ diagnostica
function showDiag() {
  const draw = () => {
    if (!location.hash.startsWith('#/diag')) return;
    const st = live.status || { drivers: info.drivers, scripts: info.scripts };
    page.innerHTML = `<div class="grid2"><div class="card"><h3>Driver</h3><table><thead><tr><th>Nome</th><th>Protocollo</th><th>Stato</th><th>Ciclo</th><th>Letture</th><th>Scritture</th><th>Errori</th></tr></thead><tbody>
      ${st.drivers.map((d) => `<tr><td>${esc(d.name)}</td><td>${esc(d.protocol)}</td><td>${d.connected ? '<span style="color:var(--ok)">connesso</span>' : `<span style="color:var(--alarm)" title="${esc(d.last_error || '')}">disconnesso</span>`}</td>
        <td>${d.cycle_ms.toFixed(1)} ms</td><td>${d.reads}</td><td>${d.writes}</td><td>${d.errors}</td></tr>
        ${d.last_error ? `<tr><td colspan=7 class="mono muted">${esc(d.last_error)}</td></tr>` : ''}`).join('')}</tbody></table></div>
      <div class="card"><h3>Script Python</h3><div class="mono">${esc(st.scripts.module)}</div>
        <p>Stato: ${st.scripts.running ? '<b style="color:var(--ok)">attivo</b>' : '<b style="color:var(--alarm)">fermo</b>'} · riavvii: ${st.scripts.restarts}</p>
        ${st.scripts.load_error ? `<pre class="mono" style="color:#fca5a5;white-space:pre-wrap">${esc(st.scripts.load_error)}</pre>` : ''}
        <div class="muted">Funzioni: ${st.scripts.functions.map(esc).join(', ')}</div></div></div>
      <div class="card"><h3>Log eventi</h3><table><tbody>${logs.slice(0, 200).map((l) => `<tr><td style="width:170px">${fmtTs(l.ts)}</td><td style="width:60px">${esc(l.level)}</td><td style="width:140px">${esc(l.source)}</td><td>${esc(l.message)}</td></tr>`).join('') || '<tr><td class="muted">Nessun evento dall\'apertura della pagina</td></tr>'}</tbody></table></div>`;
  };
  showDiag.draw = draw;
  draw();
}

// ------------------------------------------------------------ eventi live
function updateAlarmBar() {
  const bar = document.getElementById('alarmbar');
  const act = live.alarms.filter((a) => a.active);
  const unack = live.alarms.filter((a) => !a.acked).length;
  bar.classList.toggle('active', act.length > 0);
  bar.innerHTML = act.length
    ? `<span class="sev ${act[0].severity}">${act[0].severity}</span><b>${esc(act[0].message)}</b>
       <span class="muted">${fmtTs(act[0].raised_ts)}</span><span class="grow"></span>
       <span>${act.length} attivi · ${unack} da riconoscere</span><a href="#/alarms" style="color:var(--text)">Apri</a>`
    : `<span class="muted">Nessun allarme attivo</span>${unack ? `<span class="grow"></span><span>${unack} da riconoscere</span>` : ''}`;
}

window.addEventListener('hashchange', () => {});
live.on('tags', () => { renderer?.update(); popups.forEach((p) => p.renderer.update()); if (location.hash.startsWith('#/tags')) showTags.draw?.(); });
live.on('project', async () => {
  invalidateView();
  [info, views] = await Promise.all([api.get('/api/info'), api.get('/api/views')]);
  buildNav();
  refreshAreaBar();
  toast('Progetto aggiornato dal Designer');
});
live.on('alarms', () => { updateAlarmBar(); buildNav(); refreshAreaBar(); if (location.hash.startsWith('#/alarms')) showAlarms.draw?.(); });
live.on('decision', (d) => { decisions.unshift(d); decisions.length = Math.min(decisions.length, 500); if (location.hash.startsWith('#/decisions')) showDecisions.draw?.(); });
live.on('log', (l) => { logs.unshift(l); logs.length = Math.min(logs.length, 500); if (location.hash.startsWith('#/diag')) showDiag.draw?.(); });
live.on('status', () => { if (location.hash.startsWith('#/diag')) showDiag.draw?.(); });
live.on('connection', (on) => document.getElementById('conn').classList.toggle('on', on));
setInterval(() => (document.getElementById('clock').textContent = new Date().toLocaleTimeString()), 1000);

async function init() {
  info = await api.get('/api/info');
  document.title = info.plant.name;
  await initAuth('runtime.view');
  mountUserChip(document.getElementById('userChip'));
  [views, symbols] = await Promise.all([api.get('/api/views'), api.get('/api/symbols')]);
  AUTH.viewOps = Object.fromEntries(views.map((v) => [v.id, v.can_operate !== false]));
  hasWh = (await api.get('/api/wh').catch(() => [])).length > 0;
  for (const s of symbols) { try { compiled[s.id] = compileSymbol(s.svg); } catch { /* simbolo non valido */ } }
  document.getElementById('plant').textContent = info.plant.name;
  document.title = info.plant.name;
  if (!info.designer || !can('designer')) document.getElementById('designerLink').remove();
  buildNav();
  live.connect();
  refreshAreaBar();
  setInterval(refreshAreaBar, 3000);
  window.addEventListener('hashchange', () => { drawAreaBar(); });
  window.addEventListener('hashchange', route);
  route();
}
init().catch((e) => { page.innerHTML = `<div class="card">Errore di avvio: ${esc(e.message)}</div>`; });
