// Editor Python leggero senza dipendenze: evidenziazione della sintassi, numeri di
// riga, marcatori di errore, suggerimenti (Ctrl+Spazio), snippet, indentazione
// automatica e commento di riga (Ctrl+/).
import { esc } from './core.js';
import { showMenu, clipWrite, clipRead } from './ctxmenu.js';

const KW = new Set('False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case'.split(' '));
const BUILTINS = 'abs all any bool dict dir divmod enumerate filter float format getattr hasattr int isinstance len list map max min next print range repr reversed round set sorted str sum tuple type zip Exception ValueError KeyError TypeError RuntimeError'.split(' ');
const MODULES = ['math', 'random', 'time', 'json', 're', 'datetime', 'collections', 'statistics'];

/** API del contesto passato agli script (suggerimenti e documentazione). */
export const CTX_API = [
  { name: 'tag', sig: 'ctx.tag(nome, default=None)', doc: 'Valore di un tag (snapshot al momento della chiamata)', insert: 'tag("$0")' },
  { name: 'write', sig: 'ctx.write(nome, valore)', doc: 'Scrive un tag sul PLC (applicata dal WCS a fine funzione)', insert: 'write("$0", )' },
  { name: 'log', sig: 'ctx.log(messaggio)', doc: 'Messaggio nel log eventi del WCS', insert: 'log(f"$0")' },
  { name: 'tags', sig: 'ctx.tags', doc: 'dict nome_tag → valore di tutti i tag' },
  { name: 'now', sig: 'ctx.now', doc: 'Timestamp della chiamata (secondi epoch)' },
  { name: 'point', sig: 'ctx.point', doc: 'Punto di decisione / evento che ha generato la chiamata' },
  { name: 'user', sig: 'ctx.user', doc: 'Utente (eventi HMI)' },
];

/** Snippet: `$0` = cursore finale, `${1:testo}` = segnaposto selezionato. */
export const SNIPPETS = [
  { key: 'decisione', label: 'Funzione punto di decisione', body: 'def ${1:nome}(req, ctx):\n    """Punto di decisione: req contiene gli input del telegramma (+ request_id)."""\n    barcode = (req.get("barcode") or "").strip()\n    if not barcode or barcode == "NOREAD":\n        return {"destination": 0, "result": 1}\n    $0\n    return {"destination": 1, "result": 0}\n' },
  { key: 'evento', label: 'Funzione evento HMI', body: 'def ${1:nome}(payload, ctx):\n    """Evento HMI (pulsante): payload dalla vista."""\n    $0\n    return {"message": "Fatto"}\n' },
  { key: 'timer', label: 'Funzione script a timer con stato', body: '_stato = {"t": 0}\n\n\ndef ${1:nome}(payload, ctx):\n    """Script a timer: eseguito periodicamente nel processo HMI."""\n    s = _stato\n    s["t"] += 1\n    $0\n' },
  { key: 'tag', label: 'Leggi tag', body: '${1:valore} = ctx.tag("${2:Tag}", ${3:0})$0' },
  { key: 'write', label: 'Scrivi tag', body: 'ctx.write("${1:Tag}", ${2:True})$0' },
  { key: 'log', label: 'Log eventi', body: 'ctx.log(f"${1:messaggio}")$0' },
  { key: 'msg', label: 'Messaggio all\'operatore', body: 'return {"message": "${1:testo}"}$0' },
  { key: 'nav', label: 'Naviga a una vista', body: 'return {"navigate": "${1:id_vista}"}$0' },
  { key: 'popup', label: 'Apri faceplate / popup', body: 'return {"popup": "${1:fp_conv_belt}", "params": {"device": "${2:CV1}"}}$0' },
  { key: 'scarto', label: 'Esito di scarto', body: 'return {"destination": 99, "result": ${1:1}}$0' },
  { key: 'fronte', label: 'Fronte di salita di un tag', body: '_prec = {}\n\n\ndef _fronte(ctx, tag):\n    v = bool(ctx.tag(tag))\n    up = v and not _prec.get(tag, False)\n    _prec[tag] = v\n    return up\n$0' },
  { key: 'tabella', label: 'Instradamento per prefisso', body: 'ROTTE = {"${1:IT-MI}": 1, "${2:DE}": 2}\ndest = next((d for p, d in ROTTE.items() if barcode.startswith(p)), None)$0' },
  { key: 'condiviso', label: 'Importa da modulo condiviso', body: 'from _${1:modulo} import ${2:nome}$0' },
  { key: 'if', label: 'if / else', body: 'if ${1:condizione}:\n    $0\nelse:\n    pass' },
  { key: 'for', label: 'Ciclo for', body: 'for ${1:x} in ${2:elementi}:\n    $0' },
  { key: 'try', label: 'try / except', body: 'try:\n    $0\nexcept Exception as e:\n    ctx.log(f"errore: {e}")' },
  { key: 'perdispositivi', label: 'Ciclo su più dispositivi', body: 'for dev in (${1:"CV1", "CV2", "CV3"}):\n    ctx.write(f"{dev}.${2:Run}", ${3:True})$0' },
];

// ---------------------------------------------------------------- evidenziazione
const TOKEN = /(#.*$)|([rRbBuUfF]{0,2}("""|''')[\s\S]*?(\3|$))|([rRbBuUfF]{0,2}("(?:\\.|[^"\\])*"?|'(?:\\.|[^'\\])*'?))|(@[\w.]+)|(\b\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?j?\b|\b0[xXbBoO][\da-fA-F_]+\b)|([A-Za-z_]\w*)|([^\sA-Za-z_\d#"'@]+)/gm;

function highlightLine(line, state) {
  // state.tri: delimitatore di una stringa multilinea aperta
  let out = '', i = 0;
  if (state.tri) {
    const end = line.indexOf(state.tri);
    if (end < 0) return `<span class="t-str">${esc(line)}</span>`;
    out += `<span class="t-str">${esc(line.slice(0, end + 3))}</span>`;
    i = end + 3;
    state.tri = null;
  }
  let prev = '';
  const rest = line.slice(i);
  let last = 0;
  TOKEN.lastIndex = 0;
  let m;
  while ((m = TOKEN.exec(rest))) {
    if (m.index > last) out += esc(rest.slice(last, m.index));
    const t = m[0];
    let cls = '';
    if (m[1]) cls = 't-com';
    else if (m[2]) {
      cls = 't-str';
      if (!m[4]) state.tri = m[3];
    } else if (m[5]) cls = /^[fF]|^[rR][fF]/.test(t) ? 't-fstr' : 't-str';
    else if (m[7]) cls = 't-dec';
    else if (m[8]) cls = 't-num';
    else if (m[9]) {
      if (KW.has(t)) cls = t === 'True' || t === 'False' || t === 'None' ? 't-const' : 't-kw';
      else if (prev === 'def' || prev === 'class') cls = 't-def';
      else if (t === 'ctx' || t === 'self') cls = 't-ctx';
      else if (BUILTINS.includes(t)) cls = 't-bi';
      else if (rest[m.index + t.length] === '(') cls = 't-call';
      prev = t;
    } else if (m[10]) cls = 't-op';
    out += cls ? `<span class="${cls}">${esc(t)}</span>` : esc(t);
    last = m.index + t.length;
    if (!t.length) TOKEN.lastIndex++;
  }
  if (last < rest.length) out += esc(rest.slice(last));
  return out;
}

// ---------------------------------------------------------------- editor
export function createEditor(host, { value = '', onChange, onSave, onRun, hints = () => ({}) } = {}) {
  host.classList.add('pyed');
  host.innerHTML = `<div class="pyed-gutter"><div class="pyed-nums"></div></div>
    <div class="pyed-code"><pre class="pyed-hl" aria-hidden="true"></pre><textarea class="pyed-ta" spellcheck="false" autocapitalize="off" autocomplete="off" wrap="off"></textarea></div>
    <div class="pyed-pop" hidden></div><div class="pyed-tip" hidden></div>`;
  const ta = host.querySelector('.pyed-ta'), hl = host.querySelector('.pyed-hl'), nums = host.querySelector('.pyed-nums');
  const pop = host.querySelector('.pyed-pop'), tip = host.querySelector('.pyed-tip');
  ta.value = value;
  let marks = {}, sugg = [], si = 0, curLine = 0;

  const metrics = () => {
    const probe = document.createElement('span');
    probe.textContent = 'MMMMMMMMMM';
    probe.style.cssText = 'position:absolute;visibility:hidden;white-space:pre;font:inherit';
    hl.appendChild(probe);
    const w = probe.getBoundingClientRect().width / 10;
    probe.remove();
    return { cw: w, lh: parseFloat(getComputedStyle(ta).lineHeight) || 19 };
  };

  function render() {
    const lines = ta.value.split('\n');
    const state = { tri: null };
    hl.innerHTML = lines.map((l, i) => {
      const mk = marks[i + 1];
      return `<div class="ln ${i === curLine ? 'cur' : ''} ${mk ? mk.sev : ''}">${highlightLine(l, state) || ' '}</div>`;
    }).join('') + '<div class="ln"> </div>';
    nums.innerHTML = lines.map((_, i) => {
      const mk = marks[i + 1];
      return `<div class="${mk ? 'mk ' + mk.sev : ''} ${i === curLine ? 'cur' : ''}" ${mk ? `title="${esc(mk.msgs.join('\n'))}"` : ''}>${mk ? (mk.sev === 'err' ? '●' : '▲') : ''}${i + 1}</div>`;
    }).join('');
    sync();
  }
  function sync() {
    hl.style.transform = `translate(${-ta.scrollLeft}px, ${-ta.scrollTop}px)`;
    nums.style.transform = `translateY(${-ta.scrollTop}px)`;
  }
  const lineOf = (pos) => ta.value.slice(0, pos).split('\n').length - 1;
  function updateCur() {
    const l = lineOf(ta.selectionStart);
    if (l !== curLine) { curLine = l; render(); }
  }

  /** Inserisce testo mantenendo l'annullamento nativo (Ctrl+Z). */
  function insert(text, start = ta.selectionStart, end = ta.selectionEnd) {
    ta.focus();
    ta.setSelectionRange(start, end);
    if (!document.execCommand('insertText', false, text)) ta.setRangeText(text, start, end, 'end');
    changed();
  }
  function changed() { render(); onChange?.(ta.value); }

  function insertSnippet(body, start, end) {
    // ${n:testo} → testo; seleziona il primo segnaposto, altrimenti cursore su $0
    const lineStart = ta.value.lastIndexOf('\n', start - 1) + 1;
    const indent = ta.value.slice(lineStart, start).match(/^\s*/)[0];
    let txt = body.replace(/\n/g, '\n' + indent);
    let sel = null;
    txt = txt.replace(/\$\{(\d+):([^}]*)\}/g, (_, n, t, off) => { if (n === '1' && !sel) sel = [off, t.length]; return t; });
    // ricalcola gli offset dopo le sostituzioni
    const first = body.match(/\$\{1:([^}]*)\}/);
    const plain = txt.replace('$0', '');
    const cur = txt.indexOf('$0');
    insert(plain, start, end);
    if (first) {
      const idx = plain.indexOf(first[1]);
      ta.setSelectionRange(start + idx, start + idx + first[1].length);
    } else if (cur >= 0) ta.setSelectionRange(start + cur, start + cur);
  }

  // ------------------------------------------------------------ suggerimenti
  function context() {
    const pos = ta.selectionStart;
    const before = ta.value.slice(ta.value.lastIndexOf('\n', pos - 1) + 1, pos);
    let m;
    if ((m = before.match(/(?:ctx\.(?:tag|write)|\b[gw])\(\s*f?["']([^"']*)$/))) return { kind: 'tag', prefix: m[1] };
    if ((m = before.match(/ctx\.tags\[\s*["']([^"']*)$/))) return { kind: 'tag', prefix: m[1] };
    if ((m = before.match(/\bctx\.(\w*)$/))) return { kind: 'ctx', prefix: m[1] };
    if ((m = before.match(/^\s*from\s+(\w+)\s+import\s+(?:[\w\s,]*,\s*)?(\w*)$/))) return { kind: 'from', mod: m[1], prefix: m[2] };
    if ((m = before.match(/^\s*(?:from|import)\s+(\w*)$/))) return { kind: 'module', prefix: m[1] };
    if (/["'#]/.test(before.replace(/"[^"]*"|'[^']*'/g, ''))) return null; // dentro una stringa o un commento
    if ((m = before.match(/([A-Za-z_]\w*)$/))) return { kind: 'word', prefix: m[1] };
    return { kind: 'word', prefix: '' };
  }

  function collect(ctx, forced) {
    const h = hints() || {};
    const p = ctx.prefix.toLowerCase();
    const match = (s) => s.toLowerCase().startsWith(p) || (p.length > 1 && s.toLowerCase().includes(p));
    let out = [];
    if (ctx.kind === 'tag') out = (h.tags || []).filter((t) => match(t.name)).slice(0, 200).map((t) => ({ label: t.name, detail: t.type || '', doc: t.description || '', insert: t.name, kind: 'tag' }));
    else if (ctx.kind === 'ctx') out = CTX_API.filter((c) => match(c.name)).map((c) => ({ label: c.name, detail: c.sig, doc: c.doc, insert: c.insert || c.name, kind: 'ctx', snippet: !!c.insert }));
    else if (ctx.kind === 'module') out = [...(h.modules || []).map((m) => m.name), ...MODULES].filter(match).map((n) => ({ label: n, detail: n.startsWith('_') ? 'modulo condiviso' : 'libreria standard', insert: n, kind: 'mod' }));
    else if (ctx.kind === 'from') {
      const mod = (h.modules || []).find((m) => m.name === ctx.mod);
      out = (mod?.symbols || []).filter(match).map((n) => ({ label: n, detail: ctx.mod, insert: n, kind: 'sym' }));
    } else {
      if (!forced && p.length < 2) return [];
      const words = new Set(ta.value.match(/[A-Za-z_]\w{2,}/g) || []);
      words.delete(ctx.prefix);
      out = [
        ...SNIPPETS.filter((s) => match(s.key)).map((s) => ({ label: s.key, detail: 'snippet', doc: s.label, insert: s.body, kind: 'snip', snippet: true })),
        ...[...KW].filter(match).map((k) => ({ label: k, detail: 'parola chiave', insert: k, kind: 'kw' })),
        ...BUILTINS.filter(match).map((k) => ({ label: k, detail: 'builtin', insert: k, kind: 'bi' })),
        ...[...words].filter((w) => match(w) && !KW.has(w) && !BUILTINS.includes(w)).slice(0, 40).map((w) => ({ label: w, detail: 'nel file', insert: w, kind: 'id' })),
        ...(p.length ? [] : []),
      ];
      if ('ctx'.startsWith(p) && p) out.unshift({ label: 'ctx', detail: 'contesto della chiamata', insert: 'ctx', kind: 'ctx' });
    }
    // ordine: inizia con il prefisso, poi alfabetico
    out.sort((a, b) => (b.label.toLowerCase().startsWith(p) - a.label.toLowerCase().startsWith(p)) || (a.kind === 'snip' ? -1 : 0) || a.label.localeCompare(b.label));
    return out.slice(0, 60);
  }

  function showSuggest(forced = false) {
    const ctx = context();
    if (!ctx) return hideSuggest();
    sugg = collect(ctx, forced);
    if (!sugg.length) return hideSuggest();
    si = 0;
    sugg.ctx = ctx;
    const { cw, lh } = metrics();
    const pos = ta.selectionStart;
    const line = lineOf(pos), col = pos - (ta.value.lastIndexOf('\n', pos - 1) + 1) - ctx.prefix.length;
    const code = host.querySelector('.pyed-code');
    const x = code.offsetLeft + 6 + col * cw - ta.scrollLeft, y = 4 + (line + 1) * lh - ta.scrollTop;
    pop.style.left = `${Math.max(0, Math.min(x, host.clientWidth - 360))}px`;
    pop.style.top = `${y + 2 > host.clientHeight - 200 ? y - lh - Math.min(240, sugg.length * 24) - 4 : y + 2}px`;
    drawSuggest();
    pop.hidden = false;
  }
  const ICON = { tag: '🏷', ctx: 'ⓒ', mod: '📦', sym: '◆', snip: '✂', kw: 'K', bi: 'ƒ', id: '·' };
  function drawSuggest() {
    pop.innerHTML = sugg.map((s, i) => `<div class="it ${i === si ? 'on' : ''}" data-i="${i}"><span class="ic k-${s.kind}">${ICON[s.kind] || ''}</span><span class="lb">${esc(s.label)}</span><span class="dt">${esc(s.detail || '')}</span></div>`).join('')
      + (sugg[si]?.doc ? `<div class="doc">${esc(sugg[si].doc)}</div>` : '');
    pop.querySelector('.it.on')?.scrollIntoView({ block: 'nearest' });
    pop.querySelectorAll('.it').forEach((el) => (el.onmousedown = (e) => { e.preventDefault(); si = +el.dataset.i; accept(); }));
  }
  function hideSuggest() { pop.hidden = true; sugg = []; }
  function accept() {
    const s = sugg[si];
    if (!s) return;
    const end = ta.selectionStart, start = end - sugg.ctx.prefix.length;
    hideSuggest();
    if (s.snippet) insertSnippet(s.insert, start, end);
    else insert(s.insert, start, end);
  }

  // firma della funzione di ctx sotto il cursore
  function showTip() {
    const pos = ta.selectionStart;
    const before = ta.value.slice(ta.value.lastIndexOf('\n', pos - 1) + 1, pos);
    const m = before.match(/ctx\.(\w+)\([^()]*$/);
    const api = m && CTX_API.find((c) => c.name === m[1]);
    if (!api) { tip.hidden = true; return; }
    tip.innerHTML = `<b>${esc(api.sig)}</b> — ${esc(api.doc)}`;
    tip.hidden = false;
  }

  // ------------------------------------------------------------ tastiera
  ta.addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!pop.hidden) {
      if (e.key === 'ArrowDown') { e.preventDefault(); si = (si + 1) % sugg.length; return drawSuggest(); }
      if (e.key === 'ArrowUp') { e.preventDefault(); si = (si - 1 + sugg.length) % sugg.length; return drawSuggest(); }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); return accept(); }
      if (e.key === 'Escape') { e.preventDefault(); return hideSuggest(); }
    }
    if (mod && e.key === 's') { e.preventDefault(); return onSave?.(); }
    if (mod && e.key === 'Enter') { e.preventDefault(); return onRun?.(); }
    if (mod && e.key === ' ') { e.preventDefault(); return showSuggest(true); }
    if (mod && e.key === '/') { e.preventDefault(); return toggleComment(); }
    if (mod && e.key === 'd') { e.preventDefault(); return duplicateLine(); }
    if (e.key === 'Tab') {
      e.preventDefault();
      const { selectionStart: s, selectionEnd: en } = ta;
      if (s === en && !e.shiftKey) return insert('    ');
      return indentLines(e.shiftKey ? -1 : 1);
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      const pos = ta.selectionStart;
      const ls = ta.value.lastIndexOf('\n', pos - 1) + 1;
      const line = ta.value.slice(ls, pos);
      let ind = line.match(/^\s*/)[0];
      if (/:\s*(#.*)?$/.test(line)) ind += '    ';
      else if (/^\s*(return|pass|break|continue|raise)\b/.test(line)) ind = ind.slice(0, Math.max(0, ind.length - 4));
      return insert('\n' + ind);
    }
    if (e.key === 'Backspace' && ta.selectionStart === ta.selectionEnd) {
      const pos = ta.selectionStart, ls = ta.value.lastIndexOf('\n', pos - 1) + 1;
      const before = ta.value.slice(ls, pos);
      if (before.length >= 4 && /^ +$/.test(before) && before.length % 4 === 0) { e.preventDefault(); return insert('', pos - 4, pos); }
    }
    // chiusura automatica di parentesi e virgolette
    const pairs = { '(': ')', '[': ']', '{': '}', '"': '"', "'": "'" };
    if (pairs[e.key] && !mod) {
      const { selectionStart: s, selectionEnd: en } = ta;
      if (s !== en) { e.preventDefault(); const sel = ta.value.slice(s, en); insert(e.key + sel + pairs[e.key], s, en); ta.setSelectionRange(s + 1, en + 1); return; }
      const next = ta.value[s] || '';
      if ((e.key === '"' || e.key === "'") && next === e.key) { e.preventDefault(); ta.setSelectionRange(s + 1, s + 1); return; }
      if (/[\s)\]}:,]|^$/.test(next) && !((e.key === '"' || e.key === "'") && /\w/.test(ta.value[s - 1] || ''))) {
        e.preventDefault();
        insert(e.key + pairs[e.key]);
        ta.setSelectionRange(s + 1, s + 1);
        if (e.key === '"' || e.key === "'") setTimeout(() => showSuggest(), 0);
        return;
      }
    }
    if ([')', ']', '}'].includes(e.key) && ta.value[ta.selectionStart] === e.key && ta.selectionStart === ta.selectionEnd) {
      e.preventDefault(); ta.setSelectionRange(ta.selectionStart + 1, ta.selectionStart + 1);
    }
  });
  ta.addEventListener('input', (e) => {
    changed();
    if (e.inputType === 'insertText' && /[\w.]/.test(e.data || '')) showSuggest();
    else if (e.inputType?.startsWith('delete') && !pop.hidden) showSuggest();
    else hideSuggest();
    showTip();
  });
  ta.addEventListener('scroll', () => { sync(); hideSuggest(); });
  ta.addEventListener('click', () => { hideSuggest(); updateCur(); showTip(); });
  ta.addEventListener('keyup', (e) => { if (e.key.startsWith('Arrow') || e.key === 'Home' || e.key === 'End' || e.key === 'PageUp' || e.key === 'PageDown') { updateCur(); showTip(); } });
  ta.addEventListener('blur', () => setTimeout(hideSuggest, 150));
  ta.addEventListener('contextmenu', (e) => {
    if (e.shiftKey) return;
    hideSuggest();
    const has = ta.selectionStart !== ta.selectionEnd;
    const selTxt = () => ta.value.slice(ta.selectionStart, ta.selectionEnd);
    showMenu(e, [
      { label: 'Annulla', icon: '↶', key: 'Ctrl+Z', action: () => { ta.focus(); document.execCommand('undo'); changed(); } },
      { label: 'Ripeti', icon: '↷', key: 'Ctrl+Y', action: () => { ta.focus(); document.execCommand('redo'); changed(); } },
      { sep: true },
      { label: 'Taglia', icon: '✂', key: 'Ctrl+X', disabled: !has, action: async () => { await clipWrite(selTxt()); insert(''); } },
      { label: 'Copia', icon: '⧉', key: 'Ctrl+C', disabled: !has, action: () => clipWrite(selTxt()) },
      { label: 'Incolla', icon: '📋', key: 'Ctrl+V', action: async () => { const v = await clipRead(); if (v != null) insert(v); else alert('Incolla non consentito dal browser: usa Ctrl+V'); } },
      { label: 'Seleziona tutto', icon: '▦', key: 'Ctrl+A', action: () => { ta.focus(); ta.select(); } },
      { sep: true },
      { label: 'Suggerimenti', icon: '💡', key: 'Ctrl+Spazio', action: () => { ta.focus(); showSuggest(true); } },
      { label: 'Inserisci snippet', icon: '✂', sub: SNIPPETS.map((sn) => ({ label: sn.label, key: sn.key, action: () => insertSnippet(sn.body, ta.selectionStart, ta.selectionEnd) })) },
      { label: 'Commenta / decommenta', icon: '#', key: 'Ctrl+/', action: toggleComment },
      { label: 'Indenta', icon: '⇥', key: 'Tab', action: () => indentLines(1) },
      { label: 'Riduci rientro', icon: '⇤', key: 'Maiusc+Tab', action: () => indentLines(-1) },
      { label: 'Duplica riga', icon: '⎘', key: 'Ctrl+D', action: duplicateLine },
      { label: 'Vai alla riga…', icon: '↧', action: () => { const n = parseInt(prompt('Riga:') || '', 10); if (n > 0) api.gotoLine(n); } },
      { sep: true },
      onSave ? { label: 'Salva', icon: '💾', key: 'Ctrl+S', action: onSave } : null,
      onRun ? { label: 'Esegui test', icon: '▶', key: 'Ctrl+Invio', action: onRun } : null,
    ]);
  });

  function selLines() {
    const s = ta.value.lastIndexOf('\n', ta.selectionStart - 1) + 1;
    let e = ta.value.indexOf('\n', ta.selectionEnd - (ta.selectionEnd > ta.selectionStart && ta.value[ta.selectionEnd - 1] === '\n' ? 1 : 0));
    if (e < 0) e = ta.value.length;
    return [s, e];
  }
  function indentLines(dir) {
    const [s, e] = selLines();
    const lines = ta.value.slice(s, e).split('\n');
    const out = lines.map((l) => (dir > 0 ? '    ' + l : l.replace(/^ {1,4}/, ''))).join('\n');
    insert(out, s, e);
    ta.setSelectionRange(s, s + out.length);
  }
  function toggleComment() {
    const [s, e] = selLines();
    const lines = ta.value.slice(s, e).split('\n');
    const all = lines.filter((l) => l.trim()).every((l) => /^\s*#/.test(l));
    const out = lines.map((l) => (!l.trim() ? l : all ? l.replace(/^(\s*)# ?/, '$1') : l.replace(/^(\s*)/, '$1# '))).join('\n');
    insert(out, s, e);
    ta.setSelectionRange(s, s + out.length);
  }
  function duplicateLine() {
    const [s, e] = selLines();
    const txt = ta.value.slice(s, e);
    insert(txt + '\n' + txt, s, e);
  }

  render();
  const api = {
    get value() { return ta.value; },
    set value(v) { ta.value = v; marks = {}; curLine = 0; render(); },
    focus: () => ta.focus(),
    /** Marcatori: [{line, msg, sev: 'err'|'warn'}] */
    setMarks(list) {
      marks = {};
      for (const m of list || []) {
        const k = m.line || 1;
        const cur = marks[k] || { sev: 'warn', msgs: [] };
        if (m.sev === 'err') cur.sev = 'err';
        cur.msgs.push(m.msg);
        marks[k] = cur;
      }
      render();
    },
    gotoLine(n, col = 1) {
      const lines = ta.value.split('\n');
      const pos = lines.slice(0, Math.max(0, n - 1)).reduce((t, l) => t + l.length + 1, 0) + Math.max(0, col - 1);
      ta.focus();
      ta.setSelectionRange(pos, pos);
      const { lh } = metrics();
      ta.scrollTop = Math.max(0, (n - 5) * lh);
      updateCur();
    },
    insertSnippet: (body) => insertSnippet(body, ta.selectionStart, ta.selectionEnd),
    insertText: (t) => insert(t),
  };
  return api;
}
