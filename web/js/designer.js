// Designer: editor grafico delle viste HMI (stile Perspective).
import { api, Live, esc, toast } from './core.js';
import {
  COMPONENTS, COMMON_SCHEMA, ELEMENT_ATTRS, ELEMENT_ATTR_TYPES, ViewRenderer, defaultsFor, evalBinding,
  sanitizeSvg, compileSymbol, runActions, createPopup, scopeFor, viewScope, invalidateView,
} from './render.js';
import { P, loadProject, saveProject, onProject } from './project.js';
import { whCellDialog } from './render.js';
import { kvEditor, makeSplitter } from './forms.js';

const $ = (s, r = document) => r.querySelector(s);
const NS = 'http://www.w3.org/2000/svg';
const live = new Live();

const S = {
  views: [], viewId: null, view: null, dirty: false,
  sel: [], zoom: 1, snap: true, grid: 10, preview: false,
  undo: [], redo: [], clip: null,
  tags: [], symbols: [], scripts: { functions: [] },
  lib: {}, symRoot: 'Intralogistica',
};

const stage = $('#stage');
const designAction = (viewId) => (comp, ev, scope) => runActions(comp, ev, {
  live, api, toast, viewId, scope, navigate: (v, params) => { openView(v); if (params) toast(`Parametri: ${JSON.stringify(params)}`); },
  openPopup: (vid, params, title) => {
    const p = createPopup({ live, api, toast, symbols: renderer.symbols, viewId: vid, params, title, onAction: designAction(vid), onClose: () => popups.delete(p) });
    popups.add(p);
  },
});
const popups = new Set();
const renderer = new ViewRenderer(stage, live, {
  design: true, api, onWhCell: (wh, pos) => S.preview && whCellDialog(wh, pos, { api, toast }),
  preview: () => S.preview,
  onAction: (comp, ev, scope) => designAction(S.viewId)(comp, ev, scope),
});
// griglia tra sfondo e componenti
stage.insertAdjacentHTML('afterbegin', `<defs><pattern id="gridp" width="10" height="10" patternUnits="userSpaceOnUse">
  <path d="M10 0H0V10" fill="none" stroke="#ffffff12" stroke-width="1"/></pattern></defs>`);
const gridRect = document.createElementNS(NS, 'rect');
gridRect.setAttribute('fill', 'url(#gridp)');
gridRect.style.pointerEvents = 'none';
stage.insertBefore(gridRect, renderer.layer);
const overlay = document.createElementNS(NS, 'g');
stage.appendChild(overlay);

// ================================================================ utilità
const snap = (v) => (S.snap ? Math.round(v / S.grid) * S.grid : Math.round(v));
const comp = (id) => S.view?.components.find((c) => c.id === id);
const selected = () => S.sel.map(comp).filter(Boolean);
const uid = (type) => {
  let i = 1;
  while (S.view.components.some((c) => c.id === `${type}${i}`)) i++;
  return `${type}${i}`;
};

function markDirty() {
  S.dirty = true;
  $('#dirty').hidden = false;
}

function pushUndo() {
  S.undo.push(JSON.stringify(S.view));
  if (S.undo.length > 150) S.undo.shift();
  S.redo = [];
}

function undo(redo = false) {
  const from = redo ? S.redo : S.undo, to = redo ? S.undo : S.redo;
  if (!from.length) return;
  to.push(JSON.stringify(S.view));
  S.view = JSON.parse(from.pop());
  S.sel = S.sel.filter((id) => comp(id));
  markDirty();
  refreshAll();
}

/** Applica una modifica con undo e ridisegno. */
function mutate(fn, { props = true } = {}) {
  pushUndo();
  fn();
  markDirty();
  renderer.update(true);
  drawOverlay();
  if (props) renderProps();
}

function toView(e) {
  const pt = stage.createSVGPoint();
  pt.x = e.clientX; pt.y = e.clientY;
  return pt.matrixTransform(renderer.layer.getScreenCTM().inverse());
}

// ================================================================ viste
async function loadViews() {
  S.views = await api.get('/api/views');
  const ul = $('#viewList');
  const item = (v) => `<li data-id="${esc(v.id)}" class="${v.id === S.viewId ? 'on' : ''}" title="${esc(v.id)}">
    <span class="vt">${v.home ? '🏠 ' : ''}${esc(v.title)}</span>
    <span class="acts"><button class="small" data-dup title="Duplica">⧉</button><button class="small danger" data-del title="Elimina">✕</button></span></li>`;
  const pages = S.views.filter((v) => !v.faceplate), fps = S.views.filter((v) => v.faceplate);
  const openFp = fps.some((v) => v.id === S.viewId) || localStorage.getItem('designer.fpOpen') === '1';
  ul.innerHTML = (pages.map(item).join('') || '<li class="muted">Nessuna vista</li>') +
    (fps.length ? `<li class="muted" style="cursor:default;margin-top:8px"><details id="fpGroup" ${openFp ? 'open' : ''} style="width:100%"><summary>Faceplate / popup (${fps.length})</summary><ul class="list">${fps.map(item).join('')}</ul></details></li>` : '');
  ul.querySelector('#fpGroup')?.addEventListener('toggle', (e) => localStorage.setItem('designer.fpOpen', e.target.open ? '1' : '0'));
  ul.querySelectorAll('li[data-id]').forEach((li) => {
    li.onclick = (e) => {
      const id = li.dataset.id;
      if (e.target.closest('[data-del]')) return deleteView(id);
      if (e.target.closest('[data-dup]')) return duplicateView(id);
      openView(id);
    };
  });
}

async function openView(id) {
  if (S.dirty && !confirm('Ci sono modifiche non salvate. Continuare?')) return;
  try {
    S.view = await api.get(`/api/views/${encodeURIComponent(id)}`);
  } catch (e) { return toast(e.message, 'error'); }
  S.viewId = id;
  S.view.components ||= [];
  S.sel = []; S.undo = []; S.redo = []; S.dirty = false;
  $('#dirty').hidden = true;
  localStorage.setItem('designer.view', id);
  refreshAll();
  loadViews();
}

function newViewObj(title) {
  return { title, width: 1280, height: 720, background: '#0b1220', home: false, components: [] };
}

function askId(def = '') {
  const id = prompt('Id della vista (lettere, numeri, _ e -):', def);
  if (!id) return null;
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(id)) { toast('Id non valido', 'error'); return null; }
  if (S.views.some((v) => v.id === id)) { toast('Id già esistente', 'error'); return null; }
  return id;
}

async function newView() {
  const id = askId('vista' + (S.views.length + 1));
  if (!id) return;
  const title = prompt('Titolo:', id) || id;
  await api.put(`/api/views/${id}`, newViewObj(title));
  S.dirty = false;
  await loadViews();
  openView(id);
}

async function duplicateView(src) {
  const id = askId(src + '_copia');
  if (!id) return;
  const v = await api.get(`/api/views/${src}`);
  v.title += ' (copia)'; v.home = false;
  await api.put(`/api/views/${id}`, v);
  loadViews();
}

async function deleteView(id) {
  if (!confirm(`Eliminare la vista "${id}"?`)) return;
  await api.del(`/api/views/${id}`);
  if (id === S.viewId) { S.view = null; S.viewId = null; S.dirty = false; refreshAll(); }
  loadViews();
}

async function save() {
  if (!S.view) return;
  try {
    const r = await api.put(`/api/views/${S.viewId}`, S.view);
    invalidateView(S.viewId);
    S.dirty = false;
    $('#dirty').hidden = true;
    toast(r.warnings?.length ? `Salvata con avvisi:\n${r.warnings.join('\n')}` : 'Vista salvata', r.warnings?.length ? 'error' : 'ok');
    loadViews();
  } catch (e) { toast(e.message, 'error'); }
}

// ================================================================ canvas
function applyZoom() {
  if (!S.view) return;
  stage.setAttribute('width', S.view.width * S.zoom);
  stage.setAttribute('height', S.view.height * S.zoom);
  $('#zoomLbl').textContent = Math.round(S.zoom * 100) + '%';
}

function fit() {
  if (!S.view) return;
  const c = $('#center');
  S.zoom = Math.max(0.1, Math.min((c.clientWidth - 90) / S.view.width, (c.clientHeight - 90) / S.view.height, 2));
  applyZoom();
}

function refreshAll() {
  $('#empty').hidden = !!S.view;
  $('#canvas').style.visibility = S.view ? 'visible' : 'hidden';
  $('#viewTitle').textContent = S.view ? `${S.view.title}  (${S.viewId})` : '—';
  if (S.view) {
    renderer.load(S.view);
    gridRect.setAttribute('width', S.view.width);
    gridRect.setAttribute('height', S.view.height);
    gridRect.style.display = S.snap ? '' : 'none';
    applyZoom();
  }
  drawOverlay();
  renderProps();
}

const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];
function drawOverlay() {
  overlay.innerHTML = '';
  if (!S.view) return;
  const hs = 7 / S.zoom;
  for (const c of selected()) {
    const g = document.createElementNS(NS, 'g');
    g.setAttribute('transform', `translate(${c.x} ${c.y})${c.rotation ? ` rotate(${c.rotation} ${c.w / 2} ${c.h / 2})` : ''}`);
    let h = `<rect class="sel-box" x="-2" y="-2" width="${c.w + 4}" height="${c.h + 4}"/>`;
    if (S.sel.length === 1) {
      const pos = { nw: [0, 0], n: [c.w / 2, 0], ne: [c.w, 0], e: [c.w, c.h / 2], se: [c.w, c.h], s: [c.w / 2, c.h], sw: [0, c.h], w: [0, c.h / 2] };
      for (const k of HANDLES) {
        h += `<rect class="sel-handle" data-handle="${k}" x="${pos[k][0] - hs / 2}" y="${pos[k][1] - hs / 2}" width="${hs}" height="${hs}"
          style="cursor:${k}-resize" stroke-width="1"/>`;
      }
    }
    g.innerHTML = h;
    overlay.appendChild(g);
  }
}

let drag = null;
stage.addEventListener('mousedown', (e) => {
  if (!S.view || e.button !== 0) return;
  const p = toView(e);
  const handle = e.target.closest('[data-handle]');
  if (handle) {
    const c = selected()[0];
    pushUndo();
    drag = { mode: 'resize', h: handle.dataset.handle, start: p, orig: { ...c }, c };
    e.preventDefault();
    return;
  }
  if (S.preview) return; // in anteprima i click vanno ai componenti
  const g = e.target.closest('g[data-id]');
  const id = g && g.parentNode === renderer.layer ? g.dataset.id : null;
  if (id) {
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      S.sel = S.sel.includes(id) ? S.sel.filter((x) => x !== id) : [...S.sel, id];
    } else if (!S.sel.includes(id)) S.sel = [id];
    drawOverlay(); renderProps();
    pushUndo();
    drag = { mode: 'move', start: p, orig: selected().map((c) => ({ c, x: c.x, y: c.y })), moved: false };
  } else {
    if (!e.shiftKey) S.sel = [];
    const r = document.createElementNS(NS, 'rect');
    r.setAttribute('class', 'rubber');
    overlay.appendChild(r);
    drag = { mode: 'rubber', start: p, r, base: [...S.sel] };
    drawOverlay(); overlay.appendChild(r); renderProps();
  }
  e.preventDefault();
});

window.addEventListener('mousemove', (e) => {
  if (!drag) return;
  const p = toView(e);
  const dx = p.x - drag.start.x, dy = p.y - drag.start.y;
  if (drag.mode === 'move') {
    if (Math.abs(dx) + Math.abs(dy) > 1) drag.moved = true;
    for (const o of drag.orig) { o.c.x = snap(o.x + dx); o.c.y = snap(o.y + dy); }
    renderer.update(); drawOverlay();
  } else if (drag.mode === 'resize') {
    const { c, orig: o, h } = drag;
    let { x, y, w, h: hh } = o;
    if (h.includes('e')) w = Math.max(4, snap(o.w + dx));
    if (h.includes('s')) hh = Math.max(4, snap(o.h + dy));
    if (h.includes('w')) { const nx = Math.min(snap(o.x + dx), o.x + o.w - 4); w = o.w + (o.x - nx); x = nx; }
    if (h.includes('n')) { const ny = Math.min(snap(o.y + dy), o.y + o.h - 4); hh = o.h + (o.y - ny); y = ny; }
    if (e.shiftKey && o.w && o.h) { const r = o.w / o.h; if (w / hh > r) hh = Math.round(w / r); else w = Math.round(hh * r); }
    Object.assign(c, { x, y, w, h: hh });
    renderer.update(); drawOverlay();
  } else if (drag.mode === 'rubber') {
    const x = Math.min(p.x, drag.start.x), y = Math.min(p.y, drag.start.y), w = Math.abs(dx), h = Math.abs(dy);
    Object.entries({ x, y, width: w, height: h }).forEach(([k, v]) => drag.r.setAttribute(k, v));
    drag.rect = { x, y, w, h };
  }
});

window.addEventListener('mouseup', () => {
  if (!drag) return;
  if (drag.mode === 'move' && !drag.moved) S.undo.pop();
  else if (drag.mode !== 'rubber') { markDirty(); renderProps(); }
  if (drag.mode === 'rubber') {
    if (drag.rect && drag.rect.w > 2) {
      const r = drag.rect;
      const hit = S.view.components.filter((c) => c.x >= r.x && c.y >= r.y && c.x + c.w <= r.x + r.w && c.y + c.h <= r.y + r.h).map((c) => c.id);
      S.sel = [...new Set([...drag.base, ...hit])];
    }
    drawOverlay(); renderProps();
  }
  drag = null;
});

// ---------------------------------------------------------------- drop da palette / simboli / tag
const primaryProp = { conveyor: 'running', motor: 'running', lamp: 'on', value: 'value', label: 'text', button: 'active', tank: 'level', bar: 'value', rect: 'fill', ellipse: 'fill', line: 'flow', symbol: 'fill' };

function addComponent(type, x, y, extra = {}) {
  const def = COMPONENTS[type];
  const c = {
    id: uid(type), type, name: '', x: snap(x), y: snap(y), w: def.size[0], h: def.size[1], rotation: 0,
    props: {}, bindings: {}, events: {}, ...extra,
  };
  c.name = c.id;
  mutate(() => { S.view.components.push(c); S.sel = [c.id]; });
  return c;
}

$('#center').addEventListener('dragover', (e) => { if (S.view) e.preventDefault(); });
$('#center').addEventListener('drop', (e) => {
  if (!S.view) return;
  e.preventDefault();
  const p = toView(e);
  const type = e.dataTransfer.getData('wcs/component');
  const sym = e.dataTransfer.getData('wcs/symbol');
  const tag = e.dataTransfer.getData('wcs/tag');
  if (type) addComponent(type, p.x - COMPONENTS[type].size[0] / 2, p.y - COMPONENTS[type].size[1] / 2);
  else if (sym) {
    const s = renderer.symbols[sym];
    const vb = (s?.viewBox || '0 0 100 100').split(/[\s,]+/).map(Number);
    const k = 100 / Math.max(vb[2], vb[3]);
    const m = S.lib[sym];
    // dimensione nativa in scala (nastro = 30) se definita dalla libreria
    const [nw, nh] = s?.native || [Math.round(vb[2] * k), Math.round(vb[3] * k)];
    const extra = { w: nw, h: nh, props: { symbol: sym } };
    if (m) {
      // simbolo di libreria: proprietà personalizzata 'device' (istanza UDT), binding e faceplate
      extra.custom = { device: '' };
      extra.bindings = structuredClone(m.bindings || {});
      extra.events = { onClick: { actions: [{ kind: 'popup', view: m.faceplate, params: { device: '{device}' } }] } };
    }
    addComponent('symbol', p.x - nw / 2, p.y - nh / 2, extra);
  } else if (tag) {
    // su un componente esistente: collega la proprietà principale
    const g = document.elementFromPoint(e.clientX, e.clientY)?.closest('g[data-id]');
    const target = g && g.parentNode === renderer.layer ? comp(g.dataset.id) : null;
    if (target) {
      const prop = primaryProp[target.type] || 'value';
      mutate(() => { target.bindings[prop] = { kind: 'tag', tag }; S.sel = [target.id]; });
      toast(`${tag} → ${target.name}.${prop}`, 'ok');
      return;
    }
    const t = S.tags.find((x) => x.name === tag);
    if (t?.type === 'bool') addComponent('lamp', p.x, p.y, { bindings: { on: { kind: 'tag', tag } }, props: { label: tag } });
    else addComponent('value', p.x, p.y, { bindings: { value: { kind: 'tag', tag } }, props: { label: tag, unit: t?.unit || '' } });
  }
});

// ================================================================ pannello sinistro
document.querySelectorAll('.tabs button').forEach((b) => (b.onclick = () => {
  document.querySelectorAll('.tabs button').forEach((x) => x.classList.toggle('on', x === b));
  document.querySelectorAll('.tabpane').forEach((p) => (p.hidden = p.id !== `tab-${b.dataset.tab}`));
}));

$('#palette').innerHTML = Object.entries(COMPONENTS).filter(([k]) => k !== 'symbol').map(([k, d]) =>
  `<div class="pi" draggable="true" data-type="${k}"><span class="ic">${d.icon}</span>${esc(d.label)}</div>`).join('');
$('#palette').addEventListener('dragstart', (e) => e.dataTransfer.setData('wcs/component', e.target.dataset.type));
$('#palette').addEventListener('dblclick', (e) => {
  const t = e.target.closest('.pi')?.dataset.type;
  if (t && S.view) addComponent(t, 40, 40);
});

async function loadSymbols() {
  const [list, lib] = await Promise.all([api.get('/api/symbols'), api.get('/api/library').catch(() => ({}))]);
  S.symbols = list;
  S.lib = lib?.symbols || {};
  S.libUdts = lib?.udts || [];
  renderer.setSymbols(S.symbols);
  if (S.view) renderer.update(true);
  drawSymbolMenu();
}

/** Menu simboli: radici (Intralogistica / Macchine / Processo / Altri) e sottocategorie. */
function drawSymbolMenu() {
  const cat = (id) => S.lib[id]?.category || renderer.symbols[id]?.category || 'Importati';
  const roots = ['Intralogistica', 'Macchine', 'Processo', 'Altri'];
  const rootOf = (c) => (roots.includes(c.split('/')[0]) ? c.split('/')[0] : 'Altri');
  $('#symRoots').innerHTML = roots.map((r) => `<button data-root="${r}" class="${r === S.symRoot ? 'on' : ''}">${r}</button>`).join('');
  $('#symRoots').querySelectorAll('[data-root]').forEach((b) => (b.onclick = () => { S.symRoot = b.dataset.root; drawSymbolMenu(); }));
  const f = $('#symFilter').value.toLowerCase();
  const items = S.symbols.filter((s) => rootOf(cat(s.id)) === S.symRoot)
    .map((s) => ({ ...s, title: S.lib[s.id]?.title || renderer.symbols[s.id]?.title || s.id, cat: cat(s.id) }))
    .filter((s) => !f || s.title.toLowerCase().includes(f) || s.id.includes(f));
  const groups = {};
  for (const s of items) (groups[s.cat.split('/').slice(1).join('/') || s.cat] ||= []).push(s);
  $('#symList').innerHTML = Object.entries(groups).sort().map(([g, arr]) => `<details class="cat" open><summary>${esc(g)} <span class="muted">(${arr.length})</span></summary>
    <div class="symgrid">${arr.map((s) => `<div class="si" draggable="true" data-sym="${esc(s.id)}" title="${esc(s.title)}${S.lib[s.id] ? `\nUDT: ${S.lib[s.id].udt} · faceplate: ${S.lib[s.id].faceplate}` : ''}">
      <img src="/api/symbols/${encodeURIComponent(s.id)}" alt="" loading="lazy">
      <div class="nm">${esc(s.title)}${S.lib[s.id] ? '' : `<button class="small danger" data-delsym="${esc(s.id)}" title="Elimina">✕</button>`}</div></div>`).join('')}</div></details>`).join('')
    || '<p class="hint">Nessun simbolo in questa categoria.</p>';
}
$('#symFilter').oninput = () => drawSymbolMenu();
$('#symList').addEventListener('dragstart', (e) => e.dataTransfer.setData('wcs/symbol', e.target.closest('.si').dataset.sym));
$('#symList').addEventListener('click', async (e) => {
  const id = e.target.dataset.delsym;
  if (id && confirm(`Eliminare il simbolo ${id}?`)) { await api.del(`/api/symbols/${id}`); loadSymbols(); }
});
$('#svgFile').addEventListener('change', async (e) => {
  for (const f of e.target.files) {
    try {
      const clean = sanitizeSvg(await f.text());
      let id = f.name.replace(/\.svg$/i, '').replace(/[^A-Za-z0-9_-]+/g, '_').slice(0, 64) || 'simbolo';
      if (S.symbols.some((s) => s.id === id) && !confirm(`Il simbolo ${id} esiste: sovrascrivere?`)) continue;
      await api.put(`/api/symbols/${id}`, { svg: clean });
      const n = compileSymbol(clean).elements.length;
      toast(`Importato ${id}: ${n} elementi con id collegabili`, 'ok');
    } catch (err) { toast(`${f.name}: ${err.message}`, 'error'); }
  }
  e.target.value = '';
  loadSymbols();
});

async function loadTags() {
  S.tags = await api.get('/api/tags');
  $('#tagNames').innerHTML = S.tags.map((t) => `<option value="${esc(t.name)}">`).join('');
  drawTagList();
}
function drawTagList() {
  const f = $('#tagFilter').value.toLowerCase();
  $('#tagList').innerHTML = S.tags.filter((t) => t.name.toLowerCase().includes(f)).map((t) =>
    `<li draggable="true" data-tag="${esc(t.name)}" title="${esc(t.description || '')} [${esc(t.driver)} ${esc(t.address)}]">
      ${esc(t.name)}<span class="tv" data-tv="${esc(t.name)}"></span><span class="muted" style="width:100%">${esc(t.type)}${t.writable ? ' · R/W' : ''}</span></li>`).join('');
  updateTagValues();
}
function updateTagValues() {
  document.querySelectorAll('[data-tv]').forEach((el) => { el.textContent = JSON.stringify(live.value(el.dataset.tv) ?? null); });
}
$('#tagFilter').oninput = drawTagList;
$('#tagList').addEventListener('dragstart', (e) => e.dataTransfer.setData('wcs/tag', e.target.closest('li').dataset.tag));

// ================================================================ pannello proprietà
const props = $('#props');

function inputFor(type, value, opts) {
  const v = value ?? '';
  if (type === 'bool') return `<input type="checkbox" data-in ${v === true || v === 'true' ? 'checked' : ''}>`;
  if (type === 'number') return `<input type="number" step="any" data-in value="${esc(v)}">`;
  if (type === 'color') return `<span class="colorpair"><input type="color" data-color value="${/^#[0-9a-f]{6}$/i.test(v) ? v : '#000000'}"><input type="text" data-in value="${esc(v)}"></span>`;
  if (type === 'select') return `<select data-in>${opts.map((o) => `<option ${o === v ? 'selected' : ''}>${esc(o)}</option>`).join('')}</select>`;
  if (type === 'symbol') return `<select data-in><option value=""></option>${S.symbols.map((s) => `<option value="${esc(s.id)}" ${s.id === v ? 'selected' : ''}>${esc(S.lib[s.id]?.title || s.id)}</option>`).join('')}</select>`;
  if (type === 'view') return `<select data-in><option value=""></option>${S.views.map((x) => `<option value="${esc(x.id)}" ${x.id === v ? 'selected' : ''}>${esc(x.title)}${x.faceplate ? ' (faceplate)' : ''}</option>`).join('')}</select>`;
  if (type === 'json') return `<input type="text" data-in class="mono" value="${esc(typeof v === 'object' ? JSON.stringify(v) : v)}" placeholder='{"device": "{device}"}'>`;
  return `<input type="text" data-in value="${esc(v)}">`;
}

/** Scope di un componente nel Designer: parametri di default della vista + custom. */
function compScope(c) {
  return scopeFor(c, viewScope(S.view, {}));
}

function bindingSummary(b) {
  if (!b) return '';
  const src = b.kind === 'tag' ? `🏷 ${b.tag}` : `ƒ ${b.expr}`;
  const tr = b.transform?.kind && b.transform.kind !== 'none' ? ` → ${b.transform.kind}` : '';
  return src + tr;
}

/** Riga proprietà con pulsante di binding. path = percorso in props (es. "fill", "elements.body.fill"). */
function propRow(c, path, label, type, opts) {
  const b = c.bindings?.[path];
  const cur = path.split('.').reduce((o, k) => (o ? o[k] : undefined), { ...defaultsFor(c.type), ...c.props });
  let body;
  if (b) {
    const r = evalBinding(b, live, compScope(c));
    body = `<div class="bound-info" data-editbind="${esc(path)}" title="${esc(bindingSummary(b))}${r.error ? '\n' + esc(r.error) : ''}">
      ${esc(bindingSummary(b))} <span class="lv" data-live="${esc(path)}">= ${esc(JSON.stringify(r.value))}</span></div>`;
  } else body = inputFor(type, cur, opts);
  return `<div class="prow" data-path="${esc(path)}" data-type="${type}"><label title="${esc(path)}">${esc(label)}</label>${body}
    <button class="bindbtn ${b ? 'bound' : ''}" data-bind="${esc(path)}" title="Collega a tag / espressione">🔗</button></div>`;
}

function setProp(c, path, value) {
  const parts = path.split('.');
  let o = (c.props ||= {});
  for (let i = 0; i < parts.length - 1; i++) o = (o[parts[i]] ||= {});
  o[parts.at(-1)] = value;
}

function renderProps() {
  if (!S.view) { props.innerHTML = ''; return; }
  const sel = selected();
  if (sel.length === 0) return renderViewProps();
  if (sel.length > 1) return renderMultiProps(sel);
  const c = sel[0];
  const def = COMPONENTS[c.type];
  let h = `<div class="pp"><h4>${def.icon} ${esc(def.label)} <span class="grow"></span>
      <button class="small" data-z="front" title="In primo piano">⤒</button><button class="small" data-z="back" title="In fondo">⤓</button>
      <button class="small" data-act="dup" title="Duplica (Ctrl+D)">⧉</button><button class="small danger" data-act="del" title="Elimina (Canc)">✕</button></h4>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Nome</label><input data-name value="${esc(c.name || '')}"></div>
    <div class="xy">${['x', 'y', 'w', 'h'].map((k) => `<label>${k.toUpperCase()}<input type="number" data-geo="${k}" value="${Math.round(c[k])}"></label>`).join('')}</div>
    <div class="prow" style="grid-template-columns:100px 1fr;margin-top:5px"><label>Rotazione°</label><input type="number" data-geo="rotation" value="${c.rotation || 0}"></div>
    ${customSection(c)}
    <h4>Proprietà</h4>`;
  for (const [k, t, l, o] of def.schema) h += propRow(c, k, l, t, o);
  if (c.type === 'symbol') {
    const sym = renderer.symbols[c.props?.symbol];
    if (sym?.elements.length) {
      h += `<h4>Elementi SVG</h4>`;
      for (const el of sym.elements) {
        const nb = ELEMENT_ATTRS.filter((a) => c.bindings?.[`elements.${el.id}.${a}`]).length;
        h += `<details class="el" data-elid="${esc(el.id)}" ${openEls.has(el.id) ? 'open' : ''}><summary title="${esc(el.desc || '')}">#${esc(el.id)} <span class="muted">${esc(el.desc || el.tag)}</span>${nb ? ` <span class="badge" style="background:var(--ok)">${nb}</span>` : ''}</summary><div>`;
        for (const a of ELEMENT_ATTRS) {
          if (a === 'text' && el.tag !== 'text') continue;
          h += propRow(c, `elements.${el.id}.${a}`, a, ELEMENT_ATTR_TYPES[a] || 'number');
        }
        h += `</div></details>`;
      }
    } else if (sym) h += `<p class="hint">Il simbolo non ha elementi con id: aggiungi id nell'SVG per animarne le parti.</p>`;
  }
  h += `<h4>Comuni</h4>`;
  for (const [k, t, l] of COMMON_SCHEMA) h += propRow(c, k, l, t);
  h += renderEvents(c);
  props.innerHTML = h + '</div>';
  wireProps(c);
}
const openEls = new Set();

/** Proprietà personalizzate (custom): parametri passati ai binding {nome}, a viste incorporate e popup. */
function customSection(c) {
  const cu = c.custom || {};
  const fam = c.type === 'symbol' ? S.lib[c.props?.symbol]?.udt : '';
  const insts = (P.data?.udt_instances || []).filter((i) => !fam || i.type === fam).map((i) => i.name);
  let h = `<h4>Proprietà personalizzate <span class="grow"></span><button class="small" data-addcustom title="Aggiungi proprietà">+</button></h4>`;
  if (!Object.keys(cu).length) return h + `<p class="hint">Nessuna. Es. <code>device</code> = istanza UDT, usabile nei binding come <code>{device}.Run</code> e passabile ai faceplate.</p>`;
  for (const [k, v] of Object.entries(cu)) {
    h += `<div class="prow" style="grid-template-columns:100px 1fr 26px" data-custom="${esc(k)}"><label title="{${esc(k)}}">${esc(k)}</label>
      ${k === 'device'
        ? `<select data-cval><option value=""></option>${[...new Set([...insts, v].filter(Boolean))].map((n) => `<option ${n === v ? 'selected' : ''}>${esc(n)}</option>`).join('')}</select>`
        : `<input data-cval value="${esc(v)}" list="instNames">`}
      <button class="small danger" data-delcustom title="Rimuovi">✕</button></div>`;
  }
  if (fam) h += `<p class="hint">Istanze UDT di tipo <b>${esc(fam)}</b>. Crea nuove istanze nel workspace Tag e UDT.</p>`;
  return h;
}

function renderEvents(c) {
  const ev = c.events?.onClick || { actions: [] };
  let h = `<h4>Evento: click <span class="grow"></span><button class="small" data-addact>+ Azione</button></h4>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Conferma</label><input data-confirm placeholder="(nessuna)" value="${esc(ev.confirm || '')}"></div>`;
  (ev.actions || []).forEach((a, i) => {
    h += `<div class="act" data-ai="${i}"><div class="prow"><label>Tipo</label><div class="row">
      <select data-af="kind"><option value="write" ${a.kind === 'write' ? 'selected' : ''}>Scrivi tag</option>
      <option value="script" ${a.kind === 'script' ? 'selected' : ''}>Script Python</option>
      <option value="navigate" ${a.kind === 'navigate' ? 'selected' : ''}>Vai a vista</option>
      <option value="popup" ${a.kind === 'popup' ? 'selected' : ''}>Apri popup / faceplate</option>
      <option value="wh" ${a.kind === 'wh' ? 'selected' : ''}>Magazzino (cerca / prelievo / deposito)</option></select>
      <button class="small" data-amove="-1">↑</button><button class="small danger" data-adel>✕</button></div></div>`;
    if (a.kind === 'write') {
      h += `<div class="prow"><label>Tag</label><input list="tagNames" data-af="tag" value="${esc(a.tag || '')}"></div>
        <div class="prow"><label>Valore</label><input data-af="value" value="${esc(a.value ?? '')}" placeholder="valore | toggle | prompt"></div>`;
    } else if (a.kind === 'script') {
      h += `<div class="prow"><label>Funzione</label><select data-af="fn"><option></option>${S.scripts.functions.map((f) => `<option ${f === a.fn ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select></div>
        <div class="prow"><label>Payload</label><input data-af="payload" class="mono" value="${esc(a.payload || '')}" placeholder='{"chiave": 1}'></div>`;
    } else if (a.kind === 'popup') {
      h += `<div class="prow"><label>Vista</label><select data-af="view">${S.views.map((v) => `<option value="${esc(v.id)}" ${v.id === a.view ? 'selected' : ''}>${esc(v.title)}</option>`).join('')}</select></div>
        <div class="prow"><label>Parametri</label><input data-af="params" class="mono" value="${esc(JSON.stringify(a.params || {}))}" placeholder='{"device": "{device}"}' title="Valori con {nome} = proprietà personalizate del componente o parametri della vista"></div>
        <div class="prow"><label>Titolo</label><input data-af="title" value="${esc(a.title || '')}" placeholder="(titolo della vista)"></div>`;
    } else if (a.kind === 'wh') {
      h += `<div class="prow"><label>Magazzino</label><input data-af="wh" value="${esc(a.wh || '')}" placeholder="MP1"></div>
        <div class="prow"><label>Operazione</label><select data-af="op">${[['search', 'Cerca UdC'], ['outbound', 'Prelievo'], ['inbound', 'Deposito manuale']].map(([k, l]) => `<option value="${k}" ${k === a.op ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
        <div class="prow"><label>Vista esito</label><select data-af="view"><option value="">(nessuna)</option>${S.views.map((v) => `<option value="${esc(v.id)}" ${v.id === a.view ? 'selected' : ''}>${esc(v.title)}</option>`).join('')}</select></div>`;
    } else {
      h += `<div class="prow"><label>Vista</label><select data-af="view">${S.views.map((v) => `<option value="${esc(v.id)}" ${v.id === a.view ? 'selected' : ''}>${esc(v.title)}</option>`).join('')}</select></div>
        <div class="prow"><label>Parametri</label><input data-af="params" class="mono" value="${esc(JSON.stringify(a.params || {}))}" placeholder='{"aisle": "2"}'></div>`;
    }
    h += `</div>`;
  });
  return h;
}

function parseScalar(type, el) {
  if (type === 'bool') return el.checked;
  if (type === 'number') return el.value === '' ? '' : +el.value;
  return el.value;
}

function wireProps(c) {
  props.querySelector('[data-addcustom]').onclick = () => {
    const k = prompt('Nome della proprietà personalizzata (es. device):', c.custom?.device === undefined ? 'device' : '');
    if (!k || !/^[A-Za-z_]\w*$/.test(k)) return;
    mutate(() => { (c.custom ||= {})[k] = ''; });
  };
  props.querySelectorAll('[data-custom]').forEach((row) => {
    const k = row.dataset.custom;
    row.querySelector('[data-cval]').onchange = (e) => mutate(() => { c.custom[k] = e.target.value; });
    row.querySelector('[data-delcustom]').onclick = () => mutate(() => { delete c.custom[k]; });
  });
  props.querySelector('[data-name]').onchange = (e) => mutate(() => (c.name = e.target.value.trim() || c.id), { props: false });
  props.querySelectorAll('[data-geo]').forEach((el) => (el.onchange = () => mutate(() => (c[el.dataset.geo] = +el.value || 0), { props: false })));
  props.querySelectorAll('.prow[data-path]').forEach((row) => {
    const path = row.dataset.path, type = row.dataset.type;
    const inp = row.querySelector('[data-in]');
    const col = row.querySelector('[data-color]');
    if (inp) inp.onchange = () => mutate(() => setProp(c, path, parseScalar(type, inp)), { props: false });
    if (col) col.oninput = () => { inp.value = col.value; inp.onchange(); };
    row.querySelector('[data-bind]').onclick = () => openBinding(c, path);
    row.querySelector('[data-editbind]')?.addEventListener('click', () => openBinding(c, path));
  });
  props.querySelectorAll('details.el').forEach((d) => (d.ontoggle = () => (d.open ? openEls.add(d.dataset.elid) : openEls.delete(d.dataset.elid))));
  props.querySelector('[data-act="del"]').onclick = deleteSel;
  props.querySelector('[data-act="dup"]').onclick = duplicateSel;
  props.querySelectorAll('[data-z]').forEach((b) => (b.onclick = () => zorder(b.dataset.z)));
  // eventi
  const ev = () => (c.events ||= {}, c.events.onClick ||= { actions: [] });
  props.querySelector('[data-addact]').onclick = () => mutate(() => ev().actions.push({ kind: 'write', tag: '', value: 'toggle' }));
  props.querySelector('[data-confirm]').onchange = (e) => mutate(() => (ev().confirm = e.target.value), { props: false });
  props.querySelectorAll('.act').forEach((box) => {
    const i = +box.dataset.ai;
    box.querySelectorAll('[data-af]').forEach((el) => (el.onchange = () => mutate(() => {
      const a = ev().actions[i];
      if (el.dataset.af === 'params') {
        try { a.params = JSON.parse(el.value || '{}'); el.classList.remove('invalid'); } catch { el.classList.add('invalid'); toast('Parametri: JSON non valido', 'error'); }
        return;
      }
      a[el.dataset.af] = el.value;
      if (el.dataset.af === 'kind') {
        for (const k of Object.keys(a)) if (k !== 'kind') delete a[k];
        if (a.kind === 'navigate') a.view = S.views[0]?.id;
        if (a.kind === 'wh') { a.op = 'search'; a.wh = ''; }
        if (a.kind === 'popup') { a.view = S.views.find((v) => v.faceplate)?.id || S.views[0]?.id; a.params = c.custom?.device !== undefined ? { device: '{device}' } : {}; }
      }
    }, { props: el.dataset.af === 'kind' })));
    box.querySelector('[data-adel]').onclick = () => mutate(() => ev().actions.splice(i, 1));
    box.querySelector('[data-amove]').onclick = () => i > 0 && mutate(() => { const a = ev().actions; [a[i - 1], a[i]] = [a[i], a[i - 1]]; });
  });
}

function renderViewProps() {
  const v = S.view;
  props.innerHTML = `<div class="pp"><h4>Vista</h4>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Titolo</label><input data-v="title" value="${esc(v.title)}"></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Larghezza</label><input type="number" data-v="width" value="${v.width}"></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Altezza</label><input type="number" data-v="height" value="${v.height}"></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Sfondo</label><span class="colorpair"><input type="color" data-vc value="${esc(v.background || '#0b1220')}"><input data-v="background" value="${esc(v.background || '')}"></span></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Pagina iniziale</label><input type="checkbox" data-v="home" ${v.home ? 'checked' : ''}></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Area</label><select data-v="area"><option value=""></option>${(P.data?.areas || []).map((a) => `<option ${a.name === v.area ? 'selected' : ''}>${esc(a.name)}</option>`).join('')}</select></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Faceplate/popup</label><input type="checkbox" data-v="faceplate" ${v.faceplate ? 'checked' : ''} title="Nascosta dal menu del runtime, aperta come popup"></div>
    <div class="prow" style="grid-template-columns:100px 1fr"><label>Nel menu</label><input type="checkbox" data-v="menu" ${v.menu !== false ? 'checked' : ''} title="Mostra la vista nel menu del runtime (togli per sotto-pagine raggiunte via navigazione con parametri)"></div>
    <h4>Parametri della vista</h4><p class="hint">Ricevuti da popup o viste incorporate. Usali come <code>{nome}</code> nei binding (es. <code>{device}.Run</code>). Il valore qui è il default per il Designer.</p>
    <div id="vparams"></div>
    <h4>Componenti (${v.components.length})</h4>
    <ul class="list">${[...v.components].reverse().map((c) => `<li data-pick="${esc(c.id)}">${COMPONENTS[c.type]?.icon || '?'} ${esc(c.name || c.id)}
      <span class="muted">${Object.keys(c.bindings || {}).length ? '🔗' + Object.keys(c.bindings).length : ''}</span></li>`).join('')}</ul>
    <h4>Scorciatoie</h4><p class="hint">Canc elimina · Ctrl+C/V copia/incolla · Ctrl+D duplica · frecce sposta (Shift = 10px) · Shift+trascina maniglia = proporzionale · doppio click in palette = aggiungi</p></div>`;
  props.querySelectorAll('[data-v]').forEach((el) => (el.onchange = () => mutate(() => {
    const k = el.dataset.v;
    v[k] = el.type === 'checkbox' ? el.checked : el.type === 'number' ? Math.max(100, +el.value) : el.value;
    if (k === 'area' && !v.area) delete v.area;
    $('#viewTitle').textContent = `${v.title}  (${S.viewId})`;
    if (k === 'width' || k === 'height' || k === 'background') { renderer.load(v); gridRect.setAttribute('width', v.width); gridRect.setAttribute('height', v.height); applyZoom(); }
  })));
  kvEditor(props.querySelector('#vparams'), (v.params ||= {}), { onChange: () => { markDirty(); renderer.load(v); }, keyLabel: 'Parametro', valLabel: 'Default', valList: 'instNames' });
  props.querySelector('[data-vc]').oninput = (e) => { const t = props.querySelector('[data-v="background"]'); t.value = e.target.value; t.onchange(); };
  props.querySelectorAll('[data-pick]').forEach((li) => (li.onclick = () => { S.sel = [li.dataset.pick]; drawOverlay(); renderProps(); }));
}

function renderMultiProps(sel) {
  props.innerHTML = `<div class="pp"><h4>${sel.length} componenti selezionati</h4>
    <div class="toolbar">
      <button data-al="left">⇤ Sx</button><button data-al="hcenter">↔ Centro</button><button data-al="right">⇥ Dx</button>
      <button data-al="top">⤒ Alto</button><button data-al="vcenter">↕ Centro</button><button data-al="bottom">⤓ Basso</button>
      <button data-al="dh">Distribuisci ↔</button><button data-al="dv">Distribuisci ↕</button>
      <button data-al="same">Stessa dimensione</button></div>
    <h4>Azioni</h4><div class="toolbar"><button data-act="dup">Duplica</button><button class="danger" data-act="del">Elimina</button></div></div>`;
  props.querySelectorAll('[data-al]').forEach((b) => (b.onclick = () => align(b.dataset.al)));
  props.querySelector('[data-act="del"]').onclick = deleteSel;
  props.querySelector('[data-act="dup"]').onclick = duplicateSel;
}

function align(how) {
  const s = selected();
  const minX = Math.min(...s.map((c) => c.x)), maxX = Math.max(...s.map((c) => c.x + c.w));
  const minY = Math.min(...s.map((c) => c.y)), maxY = Math.max(...s.map((c) => c.y + c.h));
  mutate(() => {
    if (how === 'left') s.forEach((c) => (c.x = minX));
    if (how === 'right') s.forEach((c) => (c.x = maxX - c.w));
    if (how === 'hcenter') s.forEach((c) => (c.x = Math.round((minX + maxX) / 2 - c.w / 2)));
    if (how === 'top') s.forEach((c) => (c.y = minY));
    if (how === 'bottom') s.forEach((c) => (c.y = maxY - c.h));
    if (how === 'vcenter') s.forEach((c) => (c.y = Math.round((minY + maxY) / 2 - c.h / 2)));
    if (how === 'same') s.forEach((c) => { c.w = s[0].w; c.h = s[0].h; });
    if (how === 'dh' && s.length > 2) {
      const o = [...s].sort((a, b) => a.x - b.x), gap = (maxX - minX - o.reduce((t, c) => t + c.w, 0)) / (o.length - 1);
      let x = minX; o.forEach((c) => { c.x = Math.round(x); x += c.w + gap; });
    }
    if (how === 'dv' && s.length > 2) {
      const o = [...s].sort((a, b) => a.y - b.y), gap = (maxY - minY - o.reduce((t, c) => t + c.h, 0)) / (o.length - 1);
      let y = minY; o.forEach((c) => { c.y = Math.round(y); y += c.h + gap; });
    }
  });
}

function deleteSel() {
  if (!S.sel.length) return;
  mutate(() => { S.view.components = S.view.components.filter((c) => !S.sel.includes(c.id)); S.sel = []; });
}

function paste(list, offset = 20) {
  mutate(() => {
    S.sel = [];
    for (const src of list) {
      const c = structuredClone(src);
      c.id = uid(c.type);
      if (!c.name || c.name === src.id) c.name = c.id;
      c.x += offset; c.y += offset;
      S.view.components.push(c);
      S.sel.push(c.id);
    }
  });
}
function duplicateSel() { paste(selected()); }

function zorder(where) {
  mutate(() => {
    const sel = S.view.components.filter((c) => S.sel.includes(c.id));
    const rest = S.view.components.filter((c) => !S.sel.includes(c.id));
    S.view.components = where === 'front' ? [...rest, ...sel] : [...sel, ...rest];
  });
}

// ================================================================ modale generica
function modal(title, body, buttons, { wide = false } = {}) {
  const back = document.createElement('div');
  back.className = 'modal-back';
  back.innerHTML = `<div class="modal ${wide ? 'wide' : ''}"><div class="mh">${esc(title)}</div><div class="mb">${body}</div><div class="mf"></div></div>`;
  const close = () => back.remove();
  for (const [label, cls, fn] of buttons) {
    const b = document.createElement('button');
    b.textContent = label;
    if (cls) b.className = cls;
    b.onclick = async () => { if ((await fn?.(back)) !== false) close(); };
    back.querySelector('.mf').appendChild(b);
  }
  back.addEventListener('mousedown', (e) => { if (e.target === back) close(); });
  document.body.appendChild(back);
  return back;
}

// ================================================================ editor binding
function openBinding(c, path) {
  const b = structuredClone(c.bindings?.[path] || { kind: 'tag', tag: '' });
  b.transform ||= { kind: 'none' };
  const draw = (m) => {
    const t = b.transform;
    m.querySelector('.mb').innerHTML = `
      <div class="field"><label>Proprietà</label><div class="mono">${esc(c.name)}.${esc(path)}</div></div>
      <div class="field"><label>Sorgente</label><div class="row">
        <label class="row"><input type="radio" name="bk" value="tag" ${b.kind === 'tag' ? 'checked' : ''}> Tag</label>
        <label class="row"><input type="radio" name="bk" value="expr" ${b.kind === 'expr' ? 'checked' : ''}> Espressione</label></div></div>
      ${b.kind === 'tag'
        ? `<div class="field"><label>Tag</label><input list="tagNames" id="bTag" value="${esc(b.tag || '')}" placeholder="Nome tag"></div>`
        : `<div class="field"><label>Espressione</label><textarea id="bExpr" class="mono" rows="3" placeholder="tag('CV1.Run') && !tag('CV1.Fault')">${esc(b.expr || '')}</textarea></div>
           <p class="hint">JavaScript: <code>tag('Nome')</code> legge un tag, disponibile <code>Math</code>. Es. <code>tag('A') + tag('B')</code>, <code>tag('CV1.Fault') ? 'red' : 'green'</code></p>`}
      <div class="field"><label>Trasformazione</label><select id="bTr">
        ${[['none', 'Nessuna'], ['map', 'Mappa valori'], ['expr', 'Espressione su value'], ['format', 'Formato testo']].map(([k, l]) => `<option value="${k}" ${t.kind === k ? 'selected' : ''}>${l}</option>`).join('')}</select></div>
      ${t.kind === 'map' ? `<div class="field"><label>Regole</label><div id="bMap">
          ${(t.rules || []).map((r, i) => `<div class="maprow"><input data-mi="${i}" data-mk="in" value="${esc(r.in)}" placeholder="ingresso: 1 | true | 10..20 | >5"><input data-mi="${i}" data-mk="out" value="${esc(r.out)}" placeholder="uscita"><button class="small danger" data-mdel="${i}">✕</button></div>`).join('')}
          <button class="small" id="bMapAdd">+ Regola</button></div></div>
          <div class="field"><label>Default</label><input id="bMapDef" value="${esc(t.default ?? '')}" placeholder="se nessuna regola corrisponde (vuoto = valore originale)"></div>` : ''}
      ${t.kind === 'expr' ? `<div class="field"><label>Espressione</label><input id="bTExpr" class="mono" value="${esc(t.expr || 'value')}" placeholder="value / 10"></div>` : ''}
      ${t.kind === 'format' ? `<div class="field"><label>Formato</label><input id="bFmt" value="${esc(t.format || '{value}')}" placeholder="{value} kg"></div>
          <div class="field"><label>Decimali</label><input id="bDec" type="number" value="${esc(t.decimals ?? '')}"></div>` : ''}
      <div class="field"><label>Anteprima live</label><div class="preview-val" id="bPrev"></div></div>`;
    const mb = m.querySelector('.mb');
    mb.querySelectorAll('[name=bk]').forEach((r) => (r.onchange = () => { b.kind = r.value; draw(m); }));
    mb.querySelector('#bTag')?.addEventListener('input', (e) => (b.tag = e.target.value));
    mb.querySelector('#bExpr')?.addEventListener('input', (e) => (b.expr = e.target.value));
    mb.querySelector('#bTr').onchange = (e) => {
      b.transform = { kind: e.target.value };
      if (e.target.value === 'map') b.transform.rules = [{ in: 'true', out: '' }, { in: 'false', out: '' }];
      draw(m);
    };
    mb.querySelectorAll('[data-mi]').forEach((el) => (el.oninput = () => (t.rules[+el.dataset.mi][el.dataset.mk] = el.value)));
    mb.querySelectorAll('[data-mdel]').forEach((el) => (el.onclick = () => { t.rules.splice(+el.dataset.mdel, 1); draw(m); }));
    mb.querySelector('#bMapAdd')?.addEventListener('click', () => { t.rules.push({ in: '', out: '' }); draw(m); });
    mb.querySelector('#bMapDef')?.addEventListener('input', (e) => (t.default = e.target.value));
    mb.querySelector('#bTExpr')?.addEventListener('input', (e) => (t.expr = e.target.value));
    mb.querySelector('#bFmt')?.addEventListener('input', (e) => (t.format = e.target.value));
    mb.querySelector('#bDec')?.addEventListener('input', (e) => (t.decimals = e.target.value));
  };
  const m = modal('Binding proprietà', '', [
    ['Rimuovi binding', 'danger', () => mutate(() => { delete c.bindings[path]; })],
    ['Annulla', '', null],
    ['OK', 'primary', () => {
      if (b.kind === 'tag' && !S.tags.some((t) => t.name === b.tag)) { toast(`Tag sconosciuto: ${b.tag}`, 'error'); return false; }
      if (b.transform.kind === 'none') delete b.transform;
      if (b.kind === 'tag') delete b.expr; else delete b.tag;
      mutate(() => { (c.bindings ||= {})[path] = b; });
    }],
  ]);
  draw(m);
  const iv = setInterval(() => {
    if (!document.body.contains(m)) return clearInterval(iv);
    const r = evalBinding(b, live, compScope(c));
    const p = m.querySelector('#bPrev');
    if (p) p.innerHTML = r.error ? `<span class="err">${esc(r.error)}</span>` : `${esc(JSON.stringify(r.value))}${r.bad ? ' <span class="q-bad">(qualità non buona)</span>' : ''}`;
  }, 300);
}

// ================================================================ editor script Python
async function openScripts() {
  let src;
  try { src = await api.get('/api/scripts/source'); } catch (e) { return toast(e.message, 'error'); }
  const m = modal(`Script Python – ${src.path}`, `
    <p class="hint">Funzioni dei punti di decisione: <code>def f(req, ctx) -&gt; dict</code> · eventi HMI: <code>def f(payload, ctx)</code> ·
      <code>ctx.tag(n)</code>, <code>ctx.write(n, v)</code>, <code>ctx.log(msg)</code>. Il salvataggio verifica la sintassi e ricarica il modulo.</p>
    <textarea class="code" id="pySrc" spellcheck="false"></textarea><div class="err" id="pyErr"></div>`,
  [['Chiudi', '', null], ['Salva e ricarica', 'primary', async (mm) => {
    try {
      const st = await api.put('/api/scripts/source', { source: mm.querySelector('#pySrc').value });
      S.scripts = st;
      if (st.load_error) { mm.querySelector('#pyErr').textContent = st.load_error; return false; }
      toast(`Script ricaricato: ${st.functions.length} funzioni`, 'ok');
      renderProps();
    } catch (e) { mm.querySelector('#pyErr').textContent = e.message; return false; }
  }]], { wide: true });
  const ta = m.querySelector('#pySrc');
  ta.value = src.source;
  ta.addEventListener('keydown', (e) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const s = ta.selectionStart;
      ta.setRangeText('    ', s, ta.selectionEnd, 'end');
    }
  });
}

// ================================================================ tastiera e toolbar
window.addEventListener('keydown', (e) => {
  if (e.target.matches('input, textarea, select') || document.querySelector('.modal-back')) return;
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key === 's') { e.preventDefault(); save(); return; }
  if (!S.view) return;
  if (mod && e.key === 'z') { e.preventDefault(); undo(e.shiftKey); }
  else if (mod && e.key === 'y') { e.preventDefault(); undo(true); }
  else if (mod && e.key === 'c') S.clip = structuredClone(selected());
  else if (mod && e.key === 'v' && S.clip?.length) paste(S.clip);
  else if (mod && e.key === 'd') { e.preventDefault(); duplicateSel(); }
  else if (mod && e.key === 'a') { e.preventDefault(); S.sel = S.view.components.map((c) => c.id); drawOverlay(); renderProps(); }
  else if (e.key === 'Delete' || e.key === 'Backspace') deleteSel();
  else if (e.key === 'Escape') { S.sel = []; drawOverlay(); renderProps(); }
  else if (e.key.startsWith('Arrow') && S.sel.length) {
    e.preventDefault();
    const d = e.shiftKey ? 10 : 1;
    const [dx, dy] = { ArrowLeft: [-d, 0], ArrowRight: [d, 0], ArrowUp: [0, -d], ArrowDown: [0, d] }[e.key];
    mutate(() => selected().forEach((c) => { c.x += dx; c.y += dy; }));
  }
});
window.addEventListener('beforeunload', (e) => { if (S.dirty) { e.preventDefault(); e.returnValue = ''; } });

$('#btnSave').onclick = save;
$('#btnUndo').onclick = () => undo();
$('#btnRedo').onclick = () => undo(true);
$('#btnNewView').onclick = newView;
$('#btnZoomIn').onclick = () => { S.zoom = Math.min(4, S.zoom * 1.25); applyZoom(); drawOverlay(); };
$('#btnZoomOut').onclick = () => { S.zoom = Math.max(0.1, S.zoom / 1.25); applyZoom(); drawOverlay(); };
$('#btnFit').onclick = () => { fit(); drawOverlay(); };
$('#chkSnap').onchange = (e) => { S.snap = e.target.checked; gridRect.style.display = S.snap ? '' : 'none'; };
$('#chkPreview').onchange = (e) => { S.preview = e.target.checked; S.sel = []; drawOverlay(); renderProps(); stage.style.cursor = S.preview ? 'pointer' : ''; };
$('#btnScript').onclick = openScripts;
$('#center').addEventListener('wheel', (e) => {
  if (!e.ctrlKey || !S.view) return;
  e.preventDefault();
  S.zoom = Math.max(0.1, Math.min(4, S.zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
  applyZoom(); drawOverlay();
}, { passive: false });

// aggiornamenti live: valori dei tag nel canvas, nel pannello e nel tag browser
live.on('tags', () => {
  if (!S.view || drag) return;
  renderer.update();
  updateTagValues();
  const c = selected().length === 1 ? selected()[0] : null;
  if (c) props.querySelectorAll('[data-live]').forEach((el) => {
    const r = evalBinding(c.bindings[el.dataset.live], live, compScope(c));
    el.textContent = '= ' + JSON.stringify(r.value);
  });
});
live.on('status', (st) => { if (JSON.stringify(st.scripts.functions) !== JSON.stringify(S.scripts.functions)) S.scripts = st.scripts; });

// ================================================================ workspace
makeSplitter($('[data-split="left"]'), $('#ws-views'), '--lw', { dir: 1, min: 180, max: 520, def: 250 });
makeSplitter($('[data-split="right"]'), $('#ws-views'), '--rw', { dir: -1, min: 240, max: 720, def: 330 });
const wsLoaded = {};
async function showWorkspace(ws) {
  document.querySelectorAll('.wstabs [data-ws]').forEach((b) => b.classList.toggle('on', b.dataset.ws === ws));
  document.querySelectorAll('.ws').forEach((el) => (el.hidden = el.id !== `ws-${ws}`));
  $('#viewTools').style.display = ws === 'views' ? '' : 'none';
  localStorage.setItem('designer.ws', ws);
  if (ws === 'views') { renderProps(); return; }
  const mod = { tags: './ws_tags.js', sort: './ws_sort.js', project: './ws_project.js' }[ws];
  if (!wsLoaded[ws]) {
    wsLoaded[ws] = (await import(mod)).mount($(`#ws-${ws}`), { live, S, toast });
    // separatore del pannello laterale del workspace
    const wsl = $(`#ws-${ws} .wsl`);
    if (wsl && !wsl.querySelector('.splitter')) {
      const h = document.createElement('div');
      h.className = 'splitter';
      h.title = 'Trascina per ridimensionare (doppio click = predefinito)';
      wsl.insertBefore(h, wsl.children[1]);
      makeSplitter(h, wsl, '--sw', { dir: 1, min: 180, max: 600, def: 260, key: `split-ws-${ws}` });
    }
  }
  else wsLoaded[ws].show?.();
}
document.querySelectorAll('.wstabs [data-ws]').forEach((b) => (b.onclick = () => showWorkspace(b.dataset.ws)));
$('#btnSaveProject').onclick = async () => { if (await saveProject()) { await loadTags(); invalidateView(); } };
onProject(() => {
  $('#projDirty').hidden = !P.dirty;
  $('#instNames').innerHTML = (P.data?.udt_instances || []).map((i) => `<option value="${esc(i.name)}">`).join('');
});
window.addEventListener('beforeunload', (e) => { if (P.dirty) { e.preventDefault(); e.returnValue = ''; } });
live.on('project', () => { loadTags(); invalidateView(); });

async function init() {
  const info = await api.get('/api/info');
  if (!info.designer) { document.body.innerHTML = '<div class="card" style="margin:40px">Designer disabilitato (server.designer: false).</div>'; return; }
  S.scripts = info.scripts;
  document.title = `Designer – ${info.plant.name}`;
  await Promise.all([loadViews(), loadSymbols(), loadTags(), loadProject()]);
  live.connect();
  refreshAll();
  const last = localStorage.getItem('designer.view');
  const start = S.views.find((v) => v.id === last) || S.views.find((v) => v.home) || S.views.find((v) => !v.faceplate) || S.views[0];
  if (start) { await openView(start.id); fit(); drawOverlay(); }
  const ws = localStorage.getItem('designer.ws');
  if (ws && ws !== 'views') showWorkspace(ws);
}
export { loadTags, S as designerState };
init().catch((e) => toast(e.message, 'error'));
