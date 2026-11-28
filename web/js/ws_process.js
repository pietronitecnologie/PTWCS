// Workspace "Processo": dispositivi (unità ISA-88) e sequenze SFC (fasi) con parametri.
// Le ricette, che schedulano le fasi sui dispositivi, si gestiscono nel runtime.
import { esc } from './core.js';
import { P, markDirty, areaNames } from './project.js';
import { tableEditor } from './forms.js';
import { helpBtn } from './markdown.js';
import { stateActionsHtml, wireStateActions } from './sfc.js';
import { sfcFlow } from './sfcflow.js';

export function mount(root) {
  let tab = 'seq', sel = 0;
  root.innerHTML = `<div class="wsl"><div class="side" id="prSide"></div><div class="main" id="prMain"></div></div>`;
  const side = root.querySelector('#prSide'), main = root.querySelector('#prMain');
  const data = () => { P.data.devices ||= []; P.data.sequences ||= []; return P.data; };
  const classes = () => [...new Set(data().devices.map((d) => d.class).filter(Boolean))];

  function drawSide() {
    const d = data();
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">
        <button data-st="seq" class="${tab === 'seq' ? 'on' : ''}" style="text-align:left">🔁 Sequenze / fasi (${d.sequences.length})</button>
        <button data-st="dev" class="${tab === 'dev' ? 'on' : ''}" style="text-align:left">🏭 Dispositivi (${d.devices.length})</button></div>
      ${tab === 'seq' ? `<ul class="list">${d.sequences.map((x, i) => `<li data-i="${i}" class="${i === sel ? 'on' : ''}"><span class="vt">${esc(x.name)}</span><span class="muted" style="margin-left:auto;font-size:11px">${esc(x.device_class || '')}</span></li>`).join('')}</ul>
        <button class="small" id="prNew" style="margin-top:6px">+ Nuova sequenza</button>` : ''}
      <p class="hint">Le <b>sequenze</b> sono le fasi: SFC con parametri propri che scrivono sui tag del dispositivo (<code>{unit}</code>, <code>{parametro}</code>). Le <b>ricette</b>, che le schedulano sui dispositivi, si creano dal runtime: <b>Processo → Ricette</b>. Si applica con <b>Salva progetto</b>.</p>`;
    side.querySelectorAll('[data-st]').forEach((b) => (b.onclick = () => { tab = b.dataset.st; drawSide(); draw(); }));
    side.querySelectorAll('[data-i]').forEach((li) => (li.onclick = () => { sel = +li.dataset.i; drawSide(); draw(); }));
    side.querySelector('#prNew')?.addEventListener('click', () => {
      const n = prompt('Nome della sequenza (es. CaricoTank):');
      if (!n) return;
      d.sequences.push({ name: n.trim(), device_class: classes()[0] || '', steps: [{ name: 'Passo 1' }] });
      sel = d.sequences.length - 1; markDirty(); drawSide(); draw();
    });
  }

  function draw() { (tab === 'seq' ? drawSeq : drawDev)(); }

  function drawDev() {
    main.innerHTML = `<div class="sec"><h3>Dispositivi ${helpBtn('designer-processo#dispositivi')}</h3>
      <p class="hint">Unità di impianto (reattori, serbatoi, linee…) raggruppate per <b>classe</b>. Le sequenze lavorano su un dispositivo della loro classe; una ricetta in esecuzione (batch) <b>alloca in esclusiva</b> i dispositivi che usa.</p><div id="devTbl"></div></div>`;
    tableEditor(main.querySelector('#devTbl'), data().devices, [
      { key: 'name', label: 'Nome', width: 110, required: true }, { key: 'class', label: 'Classe', width: 130, required: true },
      { key: 'area', label: 'Area', type: 'select', options: areaNames }, { key: 'description', label: 'Descrizione', width: 260 },
    ], { onChange: () => { markDirty(); drawSide(); }, newRow: () => ({ name: `U${data().devices.length + 1}`, class: classes()[0] || 'Unità' }) });
  }

  function drawSeq() {
    const list = data().sequences;
    const s = list[sel];
    if (!s) { main.innerHTML = '<p class="muted">Nessuna sequenza. Crea la prima con “+ Nuova sequenza”.</p>'; return; }
    s.steps ||= [];
    s.parameters ||= [];
    const devs = data().devices.filter((d) => !s.device_class || d.class === s.device_class);
    main.innerHTML = `<div class="sec"><h3>Sequenza (fase) ${helpBtn('designer-processo#sequenze-fasi')}<span class="grow"></span>
        <button class="small" id="sDup">Duplica</button><button class="small danger" id="sDel">Elimina</button></h3>
      <div class="formgrid">
        <label>Nome</label><input data-f="name" value="${esc(s.name)}">
        <label>Descrizione</label><input data-f="description" value="${esc(s.description || '')}">
        <label>Area</label><select data-f="area"><option value=""></option>${areaNames().map((a) => `<option ${a === s.area ? 'selected' : ''}>${esc(a)}</option>`).join('')}</select>
        <label>Classe di dispositivo</label><div class="row"><select data-f="device_class"><option value="">(nessun dispositivo)</option>${classes().map((c) => `<option ${c === s.device_class ? 'selected' : ''}>${esc(c)}</option>`).join('')}</select><span class="hint" style="margin:0">il dispositivo sostituisce <code>{unit}</code> nei tag</span></div>
        <label>Dispositivo predefinito</label><select data-f="unit"><option value=""></option>${devs.map((d) => `<option ${d.name === s.unit ? 'selected' : ''}>${esc(d.name)}</option>`).join('')}</select>
        <label>Avvio da PLC</label><input data-f="start_tag" list="tagNames" value="${esc(s.start_tag || '')}" placeholder="tag bool (fronte di salita), sul dispositivo predefinito">
      </div></div>
      <div class="sec"><h3>Parametri ${helpBtn('designer-processo#parametri-della-sequenza')}</h3><p class="hint">Valori passati all'avvio (dalla ricetta o dall'operatore) e usati come <code>{nome}</code> in tag, valori e condizioni. Limiti verificati all'avvio.</p><div id="sPar"></div></div>
      <div class="sec"><h3>SFC della fase ${helpBtn('designer-processo#editor-sfc-a-flowchart')}</h3><div id="sSfc"></div></div>
      <div class="sec"><h3>Azioni di stato (ISA-88) ${helpBtn('designer-processo#stati-e-azioni-di-stato')}</h3>${stateActionsHtml(s)}</div>`;
    const redraw = () => { markDirty(); drawSeq(); };
    main.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
      const k = el.dataset.f;
      if (el.value === '' && k !== 'name') delete s[k]; else s[k] = el.value.trim();
      markDirty(); if (k === 'name' || k === 'device_class') { drawSide(); drawSeq(); }
    }));
    main.querySelector('#sDel').onclick = () => { if (confirm(`Eliminare la sequenza ${s.name}?`)) { list.splice(sel, 1); sel = 0; markDirty(); drawSide(); draw(); } };
    main.querySelector('#sDup').onclick = () => { const c = structuredClone(s); c.name += '_copia'; delete c.start_tag; list.push(c); sel = list.length - 1; markDirty(); drawSide(); draw(); };
    tableEditor(main.querySelector('#sPar'), s.parameters, [
      { key: 'name', label: 'Parametro', width: 120, required: true }, { key: 'unit', label: 'Unità', width: 50 },
      { key: 'min', label: 'Min', type: 'number', width: 70 }, { key: 'max', label: 'Max', type: 'number', width: 70 },
      { key: 'default', label: 'Default', width: 80 }, { key: 'description', label: 'Descrizione', width: 220 },
    ], { onChange: () => {
      for (const p of s.parameters) {
        for (const k of ['min', 'max']) if (p[k] === '' || p[k] === null || Number.isNaN(p[k])) delete p[k];
        if (p.default === '' || p.default === null) delete p.default;
        else if (typeof p.default === 'string' && !isNaN(+p.default)) p.default = +p.default;
      }
      markDirty();
    }, newRow: () => ({ name: `P${s.parameters.length + 1}` }) });
    sfcFlow(main.querySelector('#sSfc'), s.steps, {
      mode: 'sequence', onChange: markDirty,
      ctx: { functions: P.functions || [], params: () => s.parameters.map((x) => x.name), tags: () => (P.data.tags || []).map((t) => t.name) },
    });
    wireStateActions(main, s, (re) => (re ? redraw() : markDirty()));
  }

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
