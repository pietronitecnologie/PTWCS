// Workspace "Progetto": aree (gerarchia), scan class, script a timer, driver.
import { api, esc } from './core.js';
import { P, markDirty, areaNames } from './project.js';
import { tableEditor } from './forms.js';

export function mount(root, { live }) {
  let tab = 'areas';
  root.innerHTML = `<div class="wsl"><div class="side" id="pjSide"></div><div class="main" id="pjMain"></div></div>`;
  const side = root.querySelector('#pjSide'), main = root.querySelector('#pjMain');

  function drawSide() {
    side.innerHTML = `<div class="subtabs" style="flex-direction:column">${[['areas', '🗺 Aree'], ['scan', '⏱ Scan class'], ['timers', '⏲ Script a timer'], ['drivers', '🔌 Driver PLC']]
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

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
