// Runtime: pannello della simulazione fisica (collaudo virtuale).
// Avvio/arresto, fattore tempo, stato dei componenti, guasti e interventi sulle unità.
import { api, esc, toast } from './core.js';
import { can } from './auth.js';
import { helpBtn } from './markdown.js';
import { KINDS } from './physics.js';

const short = (k) => k.split('/').slice(1).join('/') || k;
const viewOf = (k) => k.split('/')[0];
const n2 = (v, d = 2) => (v === null || v === undefined ? '' : (+v).toFixed(d));

export async function showSim(page, setTimer) {
  const allowed = can('sim.operate');
  const d = allowed ? '' : 'disabled';
  page.innerHTML = `<div class="sim"><div class="row" style="margin-bottom:10px;flex-wrap:wrap;gap:8px">
      <h2 style="margin:0 12px 0 0">⚛ Simulazione fisica ${helpBtn('runtime#simulazione-fisica')}</h2>
      <span id="smState"></span><span class="grow"></span>
      <label class="muted">Fattore tempo <select id="smSpeed" ${d}>${[0.25, 0.5, 1, 2, 5, 10].map((f) => `<option value="${f}">×${f}</option>`).join('')}</select></label>
      <span id="smBtns"></span></div>
    <p class="hint" style="margin-top:0">Il PLC comanda gli attuatori (marcia nastri, deviatori, fermi); la fisica muove le unità sul layout delle viste e scrive i sensori (fotocellule, letture, finecorsa) negli ingressi simulati del PLC. Con un PLC reale il fattore tempo deve restare ×1.</p>
    <div id="smErr"></div><div class="kpis" id="smKpi"></div>
    <div class="grid2"><div class="card"><h3>Componenti</h3><div id="smComp"></div></div>
      <div class="card"><h3>Unità in impianto <span class="muted" id="smNi"></span></h3><div id="smItems" style="max-height:420px;overflow:auto"></div>
      <h3 style="margin-top:14px">Eventi</h3><div id="smLog" class="smlog"></div></div></div></div>`;
  const $ = (s) => page.querySelector(s);
  let st = null;
  const post = async (url, body = {}) => {
    try { const r = await api.post(url, body); if (r.message) toast(r.message); await load(); } catch (e) { toast(e.message, 'error'); }
  };
  const inject = (body) => post('/api/physics/inject', body);
  $('#smSpeed').onchange = (e) => post('/api/physics/speed', { factor: +e.target.value });

  function draw() {
    if (!st || !page.querySelector('.sim #smState')) return; // pagina cambiata
    const state = st.running ? (st.paused ? ['In pausa', 'warn'] : ['In corso', 'ok']) : ['Ferma', ''];
    $('#smState').innerHTML = `<span class="chip ${state[1]}">${state[0]}</span> <span class="muted">t = ${n2(st.t, 1)} s · ciclo ${st.cycle_us} µs${st.write_errors ? ` · <span style="color:var(--bad)">${st.write_errors} errori di scrittura</span>` : ''}</span>`;
    if (document.activeElement !== $('#smSpeed')) $('#smSpeed').value = String(st.speed);
    $('#smBtns').innerHTML = (!st.running ? `<button class="primary" data-c="start" ${d}>▶ Avvia</button>`
      : (st.paused ? `<button class="primary" data-c="resume" ${d}>▶ Riprendi</button>` : `<button data-c="pause" ${d}>❚❚ Pausa</button>`) + ` <button class="danger" data-c="stop" ${d}>■ Ferma</button>`)
      + ` <button data-c="reset" ${d} title="Rimuove tutte le unità e azzera il tempo">⌫ Svuota</button>`;
    $('#smBtns').querySelectorAll('[data-c]').forEach((b) => (b.onclick = () => {
      if (b.dataset.c === 'stop' && !confirm('Fermare la simulazione? Il PLC torna agli ingressi reali (watchdog).')) return;
      post(`/api/physics/${b.dataset.c}`);
    }));
    $('#smErr').innerHTML = st.errors.length ? `<div class="card" style="border-color:var(--bad)"><b>Problemi nel modello fisico</b> (i componenti indicati sono esclusi)<ul>${st.errors.map((e) => `<li>${esc(e)}</li>`).join('')}</ul></div>` : '';
    if (st.last_error) $('#smErr').innerHTML += `<p class="hint" style="color:var(--bad)">Ultimo errore di scrittura: ${esc(st.last_error)}</p>`;
    const s = st.stats;
    const views = [...new Set(st.components.map((c) => viewOf(c.key)))];
    $('#smKpi').innerHTML = [['Unità generate', s.spawned], ['Uscite', s.delivered], ['In impianto', st.items.length], ['Letture', s.reads], ['Mancate letture', s.noreads], ['Rimosse a mano', s.removed]]
      .map(([l, v]) => `<div class="kpi"><div class="muted">${l}</div><b>${v}</b></div>`).join('')
      + `<div class="kpi"><div class="muted">Viste</div>${views.map((v) => `<a href="#/view/${esc(v)}">${esc(v)}</a>`).join(' · ') || '—'}</div>`;
    // componenti per tipo
    let h = '';
    for (const [kind, k] of Object.entries(KINDS)) {
      const list = st.components.filter((c) => c.kind === kind);
      if (!list.length) continue;
      h += `<h4>${k.icon} ${k.label} (${list.length})</h4><table class="tbl"><tbody>`;
      for (const c of list) h += `<tr data-key="${esc(c.key)}"><td><b>${esc(short(c.key))}</b> <span class="muted">${esc(viewOf(c.key))}</span></td><td>${compState(c)}</td><td class="r">${compActions(c, d)}</td></tr>`;
      h += '</tbody></table>';
    }
    $('#smComp').innerHTML = h || '<p class="muted">Nessun componente con proprietà fisiche. Aggiungile nel Designer (layer ⚛ Fisica sulle viste).</p>';
    $('#smComp').querySelectorAll('[data-a]').forEach((el) => {
      const key = el.closest('[data-key]').dataset.key;
      const ev = el.tagName === 'SELECT' || el.tagName === 'INPUT' ? 'onchange' : 'onclick';
      el[ev] = () => {
        const a = el.dataset.a;
        if (a === 'photocell') inject({ action: a, key, stuck: el.value === '' ? null : el.value === 'true' });
        else if (a === 'conveyor') inject({ action: a, key, failed: el.dataset.v === '1' });
        else if (a === 'diverter' || a === 'stopper') inject({ action: a, key, stuck: el.dataset.v === '1' });
        else if (a === 'scanner') inject({ action: a, key, noread: el.dataset.v === '1' });
        else if (a === 'noread_pct') inject({ action: 'scanner', key, noread: false, noread_pct: +el.value });
        else if (a === 'source') inject({ action: a, key, enabled: el.value === '' ? null : el.value === 'true' });
        else if (a === 'interval') {
          const row = el.closest('tr');
          inject({ action: 'source', key, enabled: st.components.find((c) => c.key === key)?.enabled ?? null, interval_min: +row.querySelector('[data-imin]').value, interval_max: +row.querySelector('[data-imax]').value });
        } else if (a === 'valve' || a === 'pump' || a === 'heater' || a === 'motor') {
          const k = { valve: 'stuck', pump: 'tripped', heater: 'failed', motor: 'failed' }[a];
          inject({ action: a, key, [k]: el.dataset.v === '1' });
        } else if (a === 'leak') {
          const c = st.components.find((x) => x.key === key);
          if (c.leak) inject({ action: 'tank', key, leak: 0 });
          else { const q = prompt('Portata della perdita (m³/h):', '0.5'); if (q !== null) inject({ action: 'tank', key, leak: +q }); }
        } else if (a === 'setlevel') {
          const c = st.components.find((x) => x.key === key);
          const l = prompt('Livello (%):', String(Math.round(c.level)));
          if (l === null) return;
          const t = prompt('Temperatura (°C):', String(Math.round(c.temp)));
          inject({ action: 'tank', key, level: +l, ...(t !== null && t !== '' ? { temp: +t } : {}) });
        } else if (a === 'spawn') {
          const code = prompt('Barcode della nuova unità (vuoto = dalla sorgente):', '');
          if (code === null) return;
          inject({ action: 'spawn', key, code });
        }
      };
    });
    // unità
    $('#smNi').textContent = `(${st.items.length})`;
    $('#smItems').innerHTML = st.items.length ? `<table class="tbl"><thead><tr><th>#</th><th>Barcode</th><th>Materiale</th><th class="r">kg</th><th>Su</th><th class="r">Pos. m</th><th class="r">Età s</th><th></th></tr></thead><tbody>${
      st.items.map((i) => `<tr class="${i.jammed ? 'bad' : ''}" data-item="${i.id}"><td>${i.id}</td><td><code>${esc(i.code)}</code></td><td>${esc(i.material)}</td><td class="r">${n2(i.weight)}</td>
        <td>${esc(short(i.on))}</td><td class="r">${n2(i.front)}</td><td class="r">${n2(i.age, 1)}</td>
        <td class="r nowrap">${i.jammed ? `<button class="small" data-ia="unjam" ${d}>Libera</button>` : `<button class="small" data-ia="jam" ${d} title="Simula un'unità incastrata">Incastra</button>`} <button class="small danger" data-ia="remove" ${d}>✕</button></td></tr>`).join('')}</tbody></table>`
      : '<p class="muted">Nessuna unità.</p>';
    $('#smItems').querySelectorAll('[data-ia]').forEach((b) => (b.onclick = () => inject({ action: b.dataset.ia, item: +b.closest('[data-item]').dataset.item })));
    $('#smLog').innerHTML = st.log.map((l) => `<div><span class="muted">${n2(l.t, 1)} s</span> ${esc(l.text)}</div>`).join('') || '<span class="muted">—</span>';
  }

  function compState(c) {
    switch (c.kind) {
      case 'conveyor': return `${c.failed ? '<span class="chip err">avaria</span> ' : ''}${n2(c.v)} / ${n2(c.vmax)} m/s · ${n2(c.len)} m · ${c.items} unità${c.next ? ` → ${esc(short(c.next))}` : ''}`;
      case 'photocell': return `<span class="chip ${c.state ? 'warn' : ''}">${c.state ? 'occupata' : 'libera'}</span> ${esc(short(c.on))} @ ${n2(c.at)} m · ${esc(c.tag)}`;
      case 'scanner': return `${c.reads} letture · ultima <code>${esc(c.last || '—')}</code>`;
      case 'diverter': return `comando <b>${esc(c.cmd ?? '—')}</b> · ${c.current ? `uscita ${esc(c.current)}` : '<span class="chip warn">in commutazione / nessuna uscita</span>'}${c.stuck ? ' <span class="chip err">bloccato</span>' : ''}`;
      case 'source': return `${c.spawned} generate su ${esc(short(c.on))} · ${c.materials.join(', ')}`;
      case 'sink': return `${c.received} ricevute`;
      case 'stopper': return `${c.pos >= 0.99 ? 'fuori' : c.pos <= 0.01 ? 'dentro' : 'in corsa'}${c.stuck ? ' <span class="chip err">bloccato</span>' : ''}`;
      case 'tank': return `<span class="lvl"><i style="width:${Math.min(100, c.level)}%"></i></span> <b>${n2(c.level, 1)} %</b> · ${n2(c.volume)} / ${c.vol} m³ · ${n2(c.temp, 1)} °C · ${n2(c.pressure, 3)} bar${c.leak ? ` · <span class="chip err">perdita ${c.leak} m³/h</span>` : ''}${c.spilled > 0 ? ` · <span class="chip warn">troppo pieno ${n2(c.spilled)} m³</span>` : ''}`;
      case 'supply': return `${c.p} bar · ${c.t} °C`;
      case 'junction': return `${n2(c.p, 3)} bar`;
      case 'drain': return '';
      case 'valve': return `aperta ${n2(c.pos, 0)} % · Kv ${c.kv}${c.stuck ? ' <span class="chip err">bloccata</span>' : ''}`;
      case 'pump': return `${c.tripped ? '<span class="chip err">scattata</span> ' : ''}${c.dry ? '<span class="chip warn">a secco</span> ' : ''}velocità ${n2(c.speed, 0)} % · ${c.head} m · ${c.qmax} m³/h`;
      case 'pipe': return `${Math.abs(c.q) > 0.005 ? `<b>${n2(c.q)} m³/h</b>` : '<span class="muted">ferma</span>'}`;
      case 'heater': return `${c.on ? '<span class="chip warn">acceso</span>' : 'spento'} · ${c.kw} kW${c.failed ? ' <span class="chip err">guasto</span>' : ''}`;
      case 'motor': return `${n2(c.speed, 0)} / ${c.rpm}${c.failed ? ' <span class="chip err">avaria</span>' : ''}`;
      default: return '';
    }
  }
  function compActions(c, d) {
    const tog = (a, on, lOn, lOff) => `<button class="small ${on ? 'primary' : ''}" data-a="${a}" data-v="${on ? 0 : 1}" ${d}>${on ? lOff : lOn}</button>`;
    switch (c.kind) {
      case 'conveyor': return tog('conveyor', c.failed, '⚡ Avaria motore', 'Ripristina');
      case 'photocell': return `<select data-a="photocell" ${d}><option value="" ${c.stuck === null ? 'selected' : ''}>normale</option><option value="true" ${c.stuck === true ? 'selected' : ''}>bloccata occupata</option><option value="false" ${c.stuck === false ? 'selected' : ''}>bloccata libera</option></select>`;
      case 'scanner': return `<label class="muted">no-read % <input type="number" data-a="noread_pct" min="0" max="100" style="width:56px" value="${c.noread_pct}" ${d}></label> ${tog('scanner', c.noread, '⚡ Guasto', 'Ripristina')}`;
      case 'diverter': case 'stopper': return tog(c.kind, c.stuck, '⚡ Blocca', 'Sblocca');
      case 'valve': return tog('valve', c.stuck, '⚡ Blocca', 'Sblocca');
      case 'pump': return tog('pump', c.tripped, '⚡ Scatto termico', 'Riarma');
      case 'heater': return tog('heater', c.failed, '⚡ Guasto', 'Ripristina');
      case 'motor': return tog('motor', c.failed, '⚡ Avaria', 'Ripristina');
      case 'tank': return `<button class="small" data-a="leak" ${d}>${c.leak ? 'Chiudi perdita' : '⚡ Perdita'}</button> <button class="small" data-a="setlevel" ${d} title="Imposta livello e temperatura (prova)">Livello…</button>`;
      case 'source': return `<input type="number" step="0.1" min="0.05" data-imin style="width:56px" value="${c.interval[0]}" ${d}>–<input type="number" step="0.1" min="0.05" data-imax style="width:56px" value="${c.interval[1]}" ${d}> s <button class="small" data-a="interval" ${d}>OK</button>
        <select data-a="source" ${d}><option value="" ${c.enabled === null ? 'selected' : ''}>automatica</option><option value="true" ${c.enabled === true ? 'selected' : ''}>sempre attiva</option><option value="false" ${c.enabled === false ? 'selected' : ''}>ferma</option></select>
        <button class="small" data-a="spawn" ${d}>+1</button>`;
      default: return '';
    }
  }

  async function load() {
    st = await api.get('/api/physics');
    if (!page.querySelector('.sim')) return;
    // non ridisegnare mentre l'utente modifica un campo
    if (page.querySelector('.sim input:focus, .sim select:focus')) return;
    draw();
  }
  await load();
  setTimer(load, 1000);
}
