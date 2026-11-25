// Workspace "Script": un file per funzione, editor con evidenziazione, suggerimenti,
// snippet, controllo in tempo reale e test in un interprete isolato.
import { api, esc } from './core.js';
import { createEditor, SNIPPETS, CTX_API } from './pyeditor.js';
import { modal } from './forms.js';
import { showMenu } from './ctxmenu.js';
import { helpBtn } from './markdown.js';

const GROUPS = [
  ['decision', '🔀 Punti di decisione'], ['hmi', '🖱 Eventi HMI'], ['timer', '⏲ Script a timer'],
  ['unused', '○ Non collegati'], ['module', '📦 Moduli condivisi (_nome)'],
];

const TEMPLATES = {
  decision: (n) => `"""Punto di decisione ${n}: req contiene gli input del telegramma (+ request_id).

Ritorna un dict con le uscite, es. {"destination": 1, "result": 0}.
"""


def ${n}(req, ctx):
    barcode = (req.get("barcode") or "").strip()
    if not barcode or barcode == "NOREAD":
        ctx.log("lettura fallita")
        return {"destination": 0, "result": 1}
    return {"destination": 1, "result": 0}
`,
  hmi: (n) => `"""Evento HMI: richiamato da un pulsante o da un'azione di una vista."""


def ${n}(payload, ctx):
    ctx.log(f"${n} da {ctx.user}")
    return {"message": "Eseguito"}
`,
  timer: (n) => `"""Script a timer: eseguito periodicamente nel processo HMI."""

_stato = {"t": 0}


def ${n}(payload, ctx):
    s = _stato
    s["t"] += 1
`,
  module: () => `"""Modulo condiviso: importabile dagli script con  from _nome import ...

Lo stato definito qui è condiviso da tutti gli script che lo importano.
"""
`,
};

export function mount(root, { live, toast }) {
  root.innerHTML = `<div class="wsl"><div class="side" id="scSide"></div><div class="main sc-main" id="scMain"></div></div>`;
  const side = root.querySelector('#scSide'), main = root.querySelector('#scMain');
  let data = { files: [] }, cur = null, saved = '', ed = null, tags = [], checkT = null, points = [];

  const dirty = () => ed && cur && ed.value !== saved;
  const fileOf = (n) => data.files.find((f) => f.name === n);

  async function loadList() {
    try { data = await api.get('/api/scripts/files'); } catch (e) { side.innerHTML = `<div class="err">${esc(e.message)}</div>`; return; }
    drawSide();
  }

  function drawSide() {
    const st = data.status?.decisions || {};
    side.innerHTML = `<div class="row" style="margin-bottom:8px"><button class="primary grow" id="scNew">+ Nuovo script</button><button id="scReload" title="Ricarica tutti gli script nei processi Python">⟳</button></div>
      <input id="scFilter" placeholder="Cerca script…" style="width:100%;margin-bottom:6px">
      <div class="hint" style="margin:0 0 6px">${esc(data.dir || '')} · ${st.running ? '<span style="color:var(--ok)">● in esecuzione</span>' : '<span style="color:var(--alarm)">● fermo</span>'}</div>
      <div id="scList"></div>
      <p class="hint">${helpBtn('designer-script#un-file-per-funzione')} Un file per funzione: <code>nome.py</code> definisce <code>def nome(payload, ctx)</code>. I file <code>_nome.py</code> sono moduli condivisi.</p>`;
    side.querySelector('#scNew').onclick = newScript;
    side.querySelector('#scReload').onclick = async () => { await api.post('/api/scripts/reload'); toast('Script ricaricati', 'ok'); loadList(); };
    const flt = side.querySelector('#scFilter');
    const list = side.querySelector('#scList');
    const draw = () => {
      const q = flt.value.toLowerCase();
      list.innerHTML = GROUPS.map(([g, label]) => {
        const fs = data.files.filter((f) => f.group === g && (!q || f.name.toLowerCase().includes(q) || (f.doc || '').toLowerCase().includes(q)));
        if (!fs.length) return '';
        return `<details class="cat" open><summary>${label} <span class="muted">(${fs.length})</span></summary><ul class="list">${fs.map((f) => `
          <li data-n="${esc(f.name)}" class="${cur === f.name ? 'on' : ''}" title="${esc((f.doc || '') + (f.usage.length ? '\n\nUsato da: ' + f.usage.map((u) => u.label).join(', ') : ''))}">
            <span class="mono vt">${esc(f.name)}${cur === f.name && dirty() ? ' <span class="dirty">●</span>' : ''}</span>
            ${f.error ? '<span class="chip err" title="errore di caricamento">!</span>' : ''}</li>`).join('')}</ul></details>`;
      }).join('') || '<p class="muted">Nessuno script</p>';
      list.querySelectorAll('li[data-n]').forEach((li) => {
        li.onclick = () => open(li.dataset.n);
        li.oncontextmenu = (e) => showMenu(e, [
          { header: li.dataset.n + '.py' },
          { label: 'Apri', icon: '📝', action: () => open(li.dataset.n) },
          { label: 'Duplica…', icon: '⧉', action: () => duplicate(li.dataset.n) },
          { label: 'Elimina', icon: '🗑', danger: true, action: () => remove(li.dataset.n) },
        ]);
      });
    };
    flt.oninput = draw;
    draw();
  }

  async function open(name, { force = false } = {}) {
    if (!force && dirty() && name !== cur && !confirm(`Le modifiche a ${cur}.py non sono salvate. Continuare?`)) return;
    let r;
    try { r = await api.get(`/api/scripts/file/${encodeURIComponent(name)}`); } catch (e) { return toast(e.message, 'error'); }
    cur = name;
    saved = r.source;
    localStorage.setItem('scripts.last', name);
    drawMain();
    ed.value = r.source;
    drawSide();
    check();
  }

  function drawMain() {
    const f = fileOf(cur) || { usage: [], group: 'unused', name: cur };
    main.innerHTML = `<div class="sc-bar">
        <b class="mono">${esc(cur)}.py</b><span class="dirty" id="scDirty" hidden>●</span>
        <span class="row" style="gap:4px;flex-wrap:wrap">${f.usage.map((u) => `<span class="chip">${esc(u.label)}</span>`).join('') || (f.kind === 'module' ? '<span class="chip">modulo condiviso</span>' : '<span class="chip">non collegato</span>')}</span>
        <span class="grow"></span>
        <button id="scSnip" title="Inserisci uno snippet">✂ Snippet</button>
        <button id="scHelp" title="API del contesto ctx">ⓒ ctx</button>${helpBtn('designer-script#editor', 'Editor script')}
        <button id="scRevert" title="Annulla le modifiche non salvate">↺</button>
        <button class="primary" id="scSave" title="Controlla, salva e ricarica (Ctrl+S)">Salva</button>
      </div>
      <div class="sc-body">
        <div class="sc-edwrap"><div id="scEd" class="sc-ed"></div><div class="sc-issues" id="scIssues"></div><div class="sc-keys muted">Ctrl+Spazio suggerimenti · Tab indenta · Ctrl+/ commenta · Ctrl+D duplica riga · Ctrl+S salva · Ctrl+Invio test</div></div>
        <div class="sc-test" id="scTest"></div>
      </div>`;
    ed = createEditor(main.querySelector('#scEd'), {
      onChange: () => { main.querySelector('#scDirty').hidden = !dirty(); clearTimeout(checkT); checkT = setTimeout(check, 600); drawSideDirty(); },
      onSave: save, onRun: runTest,
      hints: () => ({ tags, modules: data.files.filter((x) => x.kind === 'module').map((x) => ({ name: x.name, symbols: x.symbols })) }),
    });
    main.querySelector('#scSave').onclick = save;
    main.querySelector('#scRevert').onclick = () => { if (dirty() && confirm('Annullare le modifiche non salvate?')) { ed.value = saved; check(); main.querySelector('#scDirty').hidden = true; drawSideDirty(); } };
    main.querySelector('#scSnip').onclick = (e) => {
      const r = e.target.getBoundingClientRect();
      showMenu({ preventDefault() {}, stopPropagation() {}, clientX: r.left, clientY: r.bottom + 4 }, SNIPPETS.map((s) => ({ label: s.label, key: s.key, icon: '✂', action: () => ed.insertSnippet(s.body) })));
    };
    main.querySelector('#scHelp').onclick = () => modal('Contesto degli script (ctx)', `<table><tbody>${CTX_API.map((c) => `<tr><td class="mono">${esc(c.sig)}</td><td>${esc(c.doc)}</td></tr>`).join('')}</tbody></table>
      <h4>Valori di ritorno</h4><ul class="hint" style="color:var(--text)">
      <li><b>Punti di decisione</b>: dict delle uscite, es. <code>{"destination": 2, "result": 0}</code>.</li>
      <li><b>Eventi HMI</b>: <code>{"message": "..."}</code>, <code>{"navigate": "vista"}</code>, <code>{"popup": "fp_x", "params": {...}}</code>.</li>
      <li><b>Script a timer</b>: nessuno; usano <code>ctx.write</code>.</li></ul>
      <p class="hint">Le <code>print()</code> vanno nel log del WCS (in test sono mostrate nel risultato). Lo stato globale dei moduli resta in memoria tra le chiamate e si azzera al salvataggio/ricarica.</p>`, [['Chiudi', '', null]], { wide: true });
    drawTest();
  }
  function drawSideDirty() {
    const li = side.querySelector(`li[data-n="${CSS.escape(cur || '')}"] .vt`);
    if (li) li.innerHTML = `${esc(cur)}${dirty() ? ' <span class="dirty">●</span>' : ''}`;
  }

  async function check() {
    if (!ed || !cur) return;
    const name = cur, src = ed.value;
    let r;
    try { r = await api.post('/api/scripts/check', { name, source: src }); } catch { return; }
    if (name !== cur || src !== ed.value) return;
    const list = [...r.errors.map((x) => ({ ...x, sev: 'err' })), ...r.warnings.map((x) => ({ ...x, sev: 'warn' }))];
    ed.setMarks(list);
    const el = main.querySelector('#scIssues');
    el.innerHTML = list.length ? list.map((x, i) => `<div class="iss ${x.sev}" data-i="${i}">${x.sev === 'err' ? '●' : '▲'} riga ${x.line}: ${esc(x.msg)}</div>`).join('') : '<div class="iss ok">✓ Nessun problema</div>';
    el.querySelectorAll('[data-i]').forEach((d) => (d.onclick = () => { const x = list[+d.dataset.i]; ed.gotoLine(x.line, x.col); }));
  }

  async function save() {
    if (!cur) return;
    try {
      const r = await api.put(`/api/scripts/file/${encodeURIComponent(cur)}`, { source: ed.value });
      if (!r.ok) {
        const e = r.check.errors[0];
        toast(`Non salvato: riga ${e?.line}: ${e?.msg}`, 'error');
        check();
        if (e) ed.gotoLine(e.line, e.col);
        return;
      }
      saved = ed.value;
      main.querySelector('#scDirty').hidden = true;
      const err = r.status.load_error;
      toast(err ? `Salvato, ma errore di caricamento:\n${err.split('\n').slice(-3).join('\n')}` : `${cur}.py salvato e ricaricato`, err ? 'error' : 'ok');
      await loadList();
    } catch (e) { toast(e.message, 'error'); }
  }

  // ------------------------------------------------------------ test
  const testKey = () => `scripts.test.${cur}`;
  function defaultPayload() {
    const f = fileOf(cur);
    const u = f?.usage.find((x) => x.kind === 'decision');
    const p = u && points.find((x) => x.name === u.point);
    if (p) {
      const keys = Array.isArray(p.inputs) ? p.inputs : Object.keys(p.inputs || {});
      return JSON.stringify(Object.fromEntries([...keys.map((k) => [k, k === 'barcode' ? 'IT-MI-0001' : 0]), ['request_id', 1]]), null, 2);
    }
    return '{}';
  }
  function drawTest() {
    const f = fileOf(cur);
    const box = main.querySelector('#scTest');
    if (f?.kind === 'module') {
      box.innerHTML = '<h4>Test</h4><p class="hint">I moduli condivisi non si eseguono direttamente: testa uno script che li importa.</p>';
      return;
    }
    const saved = JSON.parse(localStorage.getItem(testKey()) || 'null') || {};
    box.innerHTML = `<h4>Test ${helpBtn('designer-script#test')} <span class="muted" style="text-transform:none;letter-spacing:0">interprete isolato, scritture non applicate</span></h4>
      <label class="hint">Payload / req (JSON)</label><textarea id="tPayload" class="code sm" spellcheck="false">${esc(saved.payload ?? defaultPayload())}</textarea>
      <label class="hint">Tag simulati (JSON, sovrascrivono i valori live)</label><textarea id="tTags" class="code sm" spellcheck="false" placeholder='{"Line.EStop": false}'>${esc(saved.tags ?? '')}</textarea>
      <div class="row" style="margin:6px 0"><button class="primary grow" id="tRun" title="Ctrl+Invio">▶ Esegui test</button><button id="tLive" title="Inserisci i valori live dei tag usati nello script">⇩ Tag usati</button></div>
      <div id="tOut"></div>`;
    box.querySelector('#tRun').onclick = runTest;
    box.querySelector('#tLive').onclick = () => {
      const used = [...new Set([...ed.value.matchAll(/(?:ctx\.(?:tag|write)|\b[gw])\(\s*f?["']([^"'{}]+)["']/g)].map((m) => m[1]))];
      const obj = Object.fromEntries(used.map((t) => [t, live.value?.(t) ?? null]));
      box.querySelector('#tTags').value = JSON.stringify(obj, null, 2);
    };
  }

  async function runTest() {
    const box = main.querySelector('#scTest');
    const out = box.querySelector('#tOut');
    if (!out) return;
    let payload, tagsOv = {};
    const pTxt = box.querySelector('#tPayload').value, tTxt = box.querySelector('#tTags').value;
    try { payload = pTxt.trim() ? JSON.parse(pTxt) : {}; } catch (e) { out.innerHTML = `<div class="err">Payload non valido: ${esc(e.message)}</div>`; return; }
    try { tagsOv = tTxt.trim() ? JSON.parse(tTxt) : {}; } catch (e) { out.innerHTML = `<div class="err">Tag non validi: ${esc(e.message)}</div>`; return; }
    localStorage.setItem(testKey(), JSON.stringify({ payload: pTxt, tags: tTxt }));
    out.innerHTML = '<p class="muted">Esecuzione…</p>';
    let r;
    try { r = await api.post('/api/scripts/test', { name: cur, source: ed.value, payload, tags: tagsOv }); } catch (e) { out.innerHTML = `<div class="err">${esc(e.message)}</div>`; return; }
    const rep = r.reply || {};
    const where = (rep.where || []).filter((w) => w.file === `${cur}.py`);
    out.innerHTML = `<div class="t-res ${rep.ok ? 'ok' : 'ko'}">${rep.ok ? '✓ Eseguito' : '✗ Errore'}${rep.duration_ms != null ? ` <span class="muted">in ${rep.duration_ms.toFixed(2)} ms</span>` : ''}</div>
      ${rep.ok ? `<h5>Risultato</h5><pre class="t-pre">${esc(JSON.stringify(rep.result, null, 2) ?? 'None')}</pre>` : `<pre class="t-pre err">${esc(rep.error || '')}</pre>
        ${where.map((w) => `<a href="javascript:void 0" class="t-line" data-l="${w.line}">↪ ${esc(w.file)} riga ${w.line} (${esc(w.func)})</a>`).join('')}
        ${rep.trace ? `<details><summary class="hint">Traccia completa</summary><pre class="t-pre">${esc(rep.trace)}</pre></details>` : ''}`}
      ${rep.writes?.length ? `<h5>Scritture richieste (non applicate)</h5><table class="t-tbl">${rep.writes.map(([t, v]) => `<tr><td class="mono">${esc(t)}</td><td class="mono">${esc(JSON.stringify(v))}</td></tr>`).join('')}</table>` : ''}
      ${rep.logs?.length ? `<h5>ctx.log</h5><pre class="t-pre">${esc(rep.logs.join('\n'))}</pre>` : ''}
      ${rep.stdout ? `<h5>print()</h5><pre class="t-pre">${esc(rep.stdout)}</pre>` : ''}
      ${r.load_error ? `<details><summary class="hint" style="color:var(--warn)">Errori di caricamento di altri file</summary><pre class="t-pre">${esc(r.load_error)}</pre></details>` : ''}`;
    out.querySelectorAll('[data-l]').forEach((a) => (a.onclick = () => ed.gotoLine(+a.dataset.l)));
    if (!rep.ok && where[0]) ed.setMarks([{ line: where.at(-1).line, msg: rep.error, sev: 'err' }]);
  }

  // ------------------------------------------------------------ nuovo, duplica, elimina
  function newScript() {
    const back = modal('Nuovo script', `<div class="field"><label>Tipo</label><select id="nsKind">
        <option value="decision">Punto di decisione  def f(req, ctx)</option><option value="hmi">Evento HMI  def f(payload, ctx)</option>
        <option value="timer">Script a timer  def f(payload, ctx)</option><option value="module">Modulo condiviso  _nome.py</option></select></div>
      <div class="field"><label>Nome</label><input id="nsName" class="mono" placeholder="es. decidi_uscita"></div>
      <p class="hint">Il nome è un identificatore Python ed è anche il nome della funzione richiamabile da punti di decisione, timer ed eventi delle viste.</p>`, [
      ['Annulla', '', null],
      ['Crea', 'primary', async (b) => {
        const kind = b.querySelector('#nsKind').value;
        let name = b.querySelector('#nsName').value.trim();
        if (kind === 'module' && !name.startsWith('_')) name = '_' + name;
        if (!/^[A-Za-z_]\w{0,63}$/.test(name)) { toast('Nome non valido', 'error'); return false; }
        if (kind !== 'module' && name.startsWith('_')) { toast('Le funzioni non possono iniziare con "_" (riservato ai moduli)', 'error'); return false; }
        if (fileOf(name)) { toast('Esiste già', 'error'); return false; }
        try {
          const r = await api.put(`/api/scripts/file/${name}`, { source: TEMPLATES[kind](name) });
          if (!r.ok) { toast(r.check.errors[0]?.msg || 'errore', 'error'); return false; }
        } catch (e) { toast(e.message, 'error'); return false; }
        await loadList();
        open(name, { force: true });
      }]]);
    back.querySelector('#nsName').focus();
  }
  async function duplicate(name) {
    let n = prompt('Nome della copia:', name + '_copia');
    if (!n) return;
    n = n.trim();
    if (!/^[A-Za-z_]\w{0,63}$/.test(n) || fileOf(n)) return toast('Nome non valido o già esistente', 'error');
    const src = (await api.get(`/api/scripts/file/${encodeURIComponent(name)}`)).source;
    const body = name.startsWith('_') ? src : src.replace(new RegExp(`^def ${name}\\(`, 'm'), `def ${n}(`);
    const r = await api.put(`/api/scripts/file/${n}`, { source: body });
    if (!r.ok) return toast(r.check.errors[0]?.msg || 'errore', 'error');
    await loadList();
    open(n, { force: true });
  }
  async function remove(name) {
    const f = fileOf(name);
    const used = f?.usage?.length ? `\nATTENZIONE: è usato da ${f.usage.map((u) => u.label).join(', ')}.` : '';
    if (!confirm(`Eliminare ${name}.py? Una copia resta in .history.${used}`)) return;
    try { await api.del(`/api/scripts/file/${encodeURIComponent(name)}${used ? '?force=true' : ''}`); } catch (e) { return toast(e.message, 'error'); }
    if (cur === name) { cur = null; ed = null; main.innerHTML = '<p class="muted">Seleziona uno script.</p>'; }
    toast(`${name}.py eliminato`, 'ok');
    loadList();
  }

  window.addEventListener('beforeunload', (e) => { if (dirty()) { e.preventDefault(); e.returnValue = ''; } });

  (async () => {
    [tags, points] = await Promise.all([api.get('/api/tags').catch(() => []), api.get('/api/rules').catch(() => [])]);
    await loadList();
    const last = localStorage.getItem('scripts.last');
    const first = fileOf(last) ? last : data.files.find((f) => f.kind === 'function')?.name;
    if (first) open(first);
    else main.innerHTML = '<p class="muted">Nessuno script: crea il primo con “+ Nuovo script”.</p>';
  })();

  return { show: () => loadList(), openScript: (n) => open(n) };
}
