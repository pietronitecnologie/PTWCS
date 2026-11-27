// Workspace "Fisica": impostazioni del layer fisico, materiali e verifica del modello.
// Le proprietà fisiche dei singoli componenti si impostano nelle viste (layer ⚛ Fisica).
import { api, esc } from './core.js';
import { P, markDirty } from './project.js';
import { tableEditor } from './forms.js';
import { helpBtn } from './markdown.js';
import { KINDS } from './physics.js';

const DEF = { step_ms: 20, scale_mm: 10, autostart: false, drivers: [], heartbeat_tag: '', active_tag: '', materials: [] };

export function mount(root) {
  root.innerHTML = `<div class="wsl"><div class="side" id="phSide"></div><div class="main" id="phMain"></div></div>`;
  const side = root.querySelector('#phSide'), main = root.querySelector('#phMain');
  const ph = () => { P.data.physics = { ...DEF, ...(P.data.physics || {}) }; return P.data.physics; };

  function drawSide() {
    side.innerHTML = `<h3 style="margin-top:0">⚛ Layer fisico</h3>
      <p class="hint">Il layout delle viste diventa il modello fisico: su ogni componente (Viste → spunta <b>⚛ Fisica</b>) scegli il tipo — nastro, fotocellula, scanner, deviatore, sorgente, pozzo, fermo — e collega i tag.</p>
      <p class="hint"><b>▶ Attuatori</b>: tag comandati dal PLC (marcia, posizione deviatore…), letti dalla simulazione.<br><b>◀ Sensori</b>: tag scritti dalla simulazione negli ingressi simulati del PLC (fotocellule, letture, finecorsa).</p>
      <p class="hint">Per S7-1500 / PLCSIM Advanced usa il blocco <code>plc/tia/SimIO.scl</code>. ${helpBtn('designer-fisica#plc-s7-1500-e-plcsim-advanced')}</p>
      <p class="hint">Si applica con <b>Salva progetto</b>; le proprietà dei componenti con <b>Salva</b> della vista.</p>`;
  }

  async function draw() {
    const s = ph();
    const tagOpts = `<datalist id="phTags">${(P.data.tags || []).map((t) => `<option value="${esc(t.name)}">`).join('')}</datalist>`;
    main.innerHTML = `${tagOpts}<div class="sec"><h3>Impostazioni ${helpBtn('designer-fisica#impostazioni')}</h3>
      <div class="formgrid">
        <label>Passo di simulazione</label><div class="row"><input type="number" min="5" max="1000" data-f="step_ms" value="${s.step_ms}" style="width:90px"> ms</div>
        <label>Scala predefinita</label><div class="row"><input type="number" step="any" min="0.1" data-f="scale_mm" value="${s.scale_mm}" style="width:90px"> mm per pixel <span class="hint" style="margin:0">(ogni vista può avere la sua)</span></div>
        <label>Avvio automatico</label><label class="row"><input type="checkbox" data-f="autostart" ${s.autostart ? 'checked' : ''}> avvia la simulazione all'avvio del server</label>
        <label>Driver simulabili</label><div>${P.drivers.map((d) => `<label class="chip"><input type="checkbox" data-drv="${esc(d.name)}" ${s.drivers.includes(d.name) || d.protocol === 'sim' ? 'checked' : ''} ${d.protocol === 'sim' ? 'disabled title="I driver interni (sim) sono sempre ammessi"' : ''}> ${esc(d.name)} <span class="muted">${esc(d.protocol || '')}</span></label>`).join('')}
          <p class="hint" style="margin:4px 0 0">La simulazione scrive i sensori solo sui driver spuntati: <b>solo PLC virtuali o in collaudo</b> (PLCSIM Advanced), mai un PLC di produzione.</p></div>
        <label>Tag heartbeat</label><input list="phTags" data-f="heartbeat_tag" value="${esc(s.heartbeat_tag)}" placeholder="es. SimIO.Heartbeat (contatore ogni 0,5 s)">
        <label>Tag simulazione attiva</label><input list="phTags" data-f="active_tag" value="${esc(s.active_tag)}" placeholder="es. SimIO.Active">
      </div></div>
      <div class="sec"><h3>Materiali ${helpBtn('designer-fisica#materiali')}</h3><p class="hint">Unità di carico generate dalle sorgenti. Senza materiali si usa un collo standard 0,4 × 0,3 m.</p><div id="phMat"></div></div>
      <div class="sec"><h3>Verifica del modello ${helpBtn('designer-fisica#verifica-del-modello')}<span class="grow"></span><button class="small" id="phCheck">⟳ Verifica</button></h3>
        <p class="hint">Modello compilato dalle viste <b>salvate</b> e dal progetto <b>applicato</b>.</p><div id="phModel"><span class="muted">…</span></div></div>`;
    main.querySelectorAll('[data-f]').forEach((el) => (el.onchange = () => {
      const k = el.dataset.f;
      s[k] = el.type === 'checkbox' ? el.checked : el.type === 'number' ? +el.value : el.value.trim();
      markDirty();
    }));
    main.querySelectorAll('[data-drv]').forEach((el) => (el.onchange = () => {
      s.drivers = [...main.querySelectorAll('[data-drv]:checked:not(:disabled)')].map((x) => x.dataset.drv);
      markDirty();
    }));
    tableEditor(main.querySelector('#phMat'), s.materials, [
      { key: 'name', label: 'Nome', width: 110, required: true }, { key: 'length_m', label: 'Lungh. m', type: 'number', width: 70 },
      { key: 'width_m', label: 'Largh. m', type: 'number', width: 70 }, { key: 'mass_min', label: 'Massa min kg', type: 'number', width: 70 },
      { key: 'mass_max', label: 'Massa max kg', type: 'number', width: 70 }, { key: 'share', label: 'Peso nel mix', type: 'number', width: 60 },
      { key: 'color', label: 'Colore', width: 80 }, { key: 'description', label: 'Descrizione', width: 180 },
    ], {
      onChange: () => {
        for (const m of s.materials) for (const k of ['length_m', 'width_m', 'mass_min', 'mass_max', 'share']) if (m[k] === '' || m[k] === null || Number.isNaN(m[k])) delete m[k];
        markDirty();
      },
      newRow: () => ({ name: `Materiale${s.materials.length + 1}`, length_m: 0.4, width_m: 0.3, mass_min: 1, mass_max: 20, color: '#c8915a' }),
    });
    main.querySelector('#phCheck').onclick = check;
    check();
  }

  async function check() {
    const host = main.querySelector('#phModel');
    try {
      const st = await api.get('/api/physics');
      const by = {};
      for (const c of st.components) (by[c.key.split('/')[0]] ||= []).push(c);
      host.innerHTML = (st.errors.length ? `<div class="err-box"><b>${st.errors.length} problemi</b><ul>${st.errors.map((e) => `<li>${esc(e)}</li>`).join('')}</ul></div>` : '<p style="color:var(--ok)">✓ Nessun problema.</p>')
        + (Object.keys(by).length ? Object.entries(by).map(([v, list]) => `<h4>Vista <b>${esc(v)}</b></h4><table class="edit"><tbody>${list.map((c) => `<tr><td>${KINDS[c.kind]?.icon || ''} ${esc(KINDS[c.kind]?.label || c.kind)}</td><td><b>${esc(c.key.split('/').slice(1).join('/'))}</b></td><td class="muted">${esc(desc(c))}</td></tr>`).join('')}</tbody></table>`).join('')
          : '<p class="muted">Nessun componente con proprietà fisiche nelle viste.</p>')
        + `<p class="hint">Stato: ${st.running ? '<b>simulazione in corso</b>' : 'ferma'} · runtime → <b>Simulazione</b> per avviarla.</p>`;
    } catch (e) { host.innerHTML = `<p class="muted">${esc(e.message)}</p>`; }
  }
  const desc = (c) => ({
    conveyor: () => `${(+c.len).toFixed(2)} m · ${c.vmax} m/s · marcia ${c.run}${c.next ? ` → ${c.next.split('/').pop()}` : ''}`,
    photocell: () => `su ${c.on.split('/').pop()} a ${(+c.at).toFixed(2)} m → ${c.tag}`,
    scanner: () => `su ${c.on.split('/').pop()} a ${(+c.at).toFixed(2)} m`,
    diverter: () => c.outputs.map((o) => `${o.value}→${o.next.split('/').pop()}`).join(' · '),
    source: () => `su ${c.on.split('/').pop()} · ogni ${c.interval[0]}–${c.interval[1]} s · ${c.materials.join(', ')}`,
    stopper: () => `su ${c.on.split('/').pop()} a ${(+c.at).toFixed(2)} m`,
    sink: () => '',
  }[c.kind] || (() => ''))();

  drawSide();
  draw();
  return { show: () => { drawSide(); draw(); } };
}
