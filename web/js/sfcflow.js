// Editor SFC grafico stile PCS7: flowchart verticale con passi, transizioni, alternative
// (salti), rami paralleli. Interattivo: clic per selezionare, "+" sui collegamenti per
// inserire, menu del tasto destro, barra strumenti e pannello proprietà.
// Lo stesso disegno, in sola lettura, mostra lo stato di esecuzione nel runtime.
import { esc } from './core.js';
import { showMenu } from './ctxmenu.js';
import { dur, actText } from './sfc.js';

const OPS = ['==', '!=', '>', '>=', '<', '<='];
const SW = 190, SH = 50, TG = 46, ADY = 30, LBL = 250, PAD = 26, BRH = 22;
const short = (s, n = 42) => (s.length > n ? s.slice(0, n - 1) + '…' : s);
const valStr = (v) => (v === null || v === undefined ? '—' : typeof v === 'boolean' ? (v ? 'vero' : 'falso') : String(v));
const condText = (c) => `${c.tag === '@elapsed' ? 't' : c.tag === '@confirm' ? 'conferma' : c.tag === '@phases_done' ? 'fasi concluse' : c.tag || '?'}${c.tag === '@confirm' || c.tag === '@phases_done' ? '' : ` ${c.op} ${typeof c.value === 'string' && c.value.startsWith('tag:') ? c.value.slice(4) : valStr(c.value)}${c.tag === '@elapsed' ? ' s' : ''}`}`;
const trText = (t) => (t.label ? `${t.label}: ` : '') + ((t.when || []).map(condText).join(t.mode === 'any' ? ' ∨ ' : ' ∧ ') || 'vero');

/** Lista di passi al percorso ("" radice, "2.1" = ramo 1 del passo 2, "2.1/3.2" …). */
function listAt(steps, lp) {
  if (!lp) return steps;
  let list = steps;
  for (const seg of lp.split('/')) {
    const [si, bi] = seg.split('.').map(Number);
    list = list?.[si - 1]?.branches?.[bi - 1];
  }
  return list || [];
}
const parentOf = (p) => { const k = p.lastIndexOf('/'); return { lp: k < 0 ? '' : p.slice(0, k), i: +p.slice(k + 1) - 1 }; };
const stepAt = (steps, p) => { const { lp, i } = parentOf(p); return { list: listAt(steps, lp), i, st: listAt(steps, lp)[i] }; };
const uniq = (list, base) => { let n = list.length + 1; while (list.some((s) => s.name === `${base} ${n}`)) n++; return `${base} ${n}`; };

// ================================================================ disegno
function measure(list) {
  let w = SW + 20 + LBL, h = 0;
  for (const st of list) {
    h += SH;
    if (st.branches?.length) {
      const ms = st.branches.map(measure);
      w = Math.max(w, ms.reduce((a, m) => a + m.w + PAD, 0));
      h += BRH + 34 + Math.max(...ms.map((m) => m.h));
    }
    h += TG + Math.max(0, (st.transitions || []).length - 1) * ADY;
  }
  return { w, h };
}

/** SVG del flowchart. `o`: { sel, run, edit } */
function draw(list, lp, x0, y, o, out, root) {
  const cx = x0 + SW / 2 + 10;
  const sub = o.run?.subst || ((s) => s);
  const names = new Set(list.map((s) => s.name));
  const targets = {};
  list.forEach((s) => (s.transitions || []).forEach((t) => { if (t.next && t.next !== 'END') (targets[t.next] ||= []).push(s.name); }));
  const ins = (yy, idx) => o.edit && out.push(`<g class="sfx-ins" data-ins="${lp}|${idx}"><circle cx="${cx - 26}" cy="${yy}" r="8"/><text x="${cx - 26}" y="${yy + 4}" text-anchor="middle">+</text><title>Inserisci un passo qui</title></g>`);
  ins(y - 8, 0);
  let flows = true; // il passo precedente prosegue verso questo
  list.forEach((st, i) => {
    const p = lp ? `${lp}/${i + 1}` : String(i + 1);
    const act = o.run?.active?.has(p), past = o.run?.hist?.[p];
    const sel = o.sel?.type === 's' && o.sel.path === p;
    if (!flows) out.push(`<line x1="${cx}" y1="${y - 10}" x2="${cx}" y2="${y}" class="sfx-ln dash"/>`);
    if (targets[st.name]) out.push(`<text x="${cx + SW / 2 + 6}" y="${y + 12}" class="sfx-jt">◀ da ${esc(short(targets[st.name].join(', '), 30))}</text>`);
    // passo
    const cls = `sfx-step${act ? ' cur' : past ? ' past' : ''}${sel ? ' sel' : ''}`;
    const nAct = (st.actions || []).length;
    const info = [nAct ? `⚡${nAct}` : '', st.branches?.length ? `⫴${st.branches.length}` : '', st.min_time_s ? `⧗${st.min_time_s}s` : '', st.timeout_s ? `⏱${st.timeout_s}s` : ''].filter(Boolean).join('  ');
    const phases = (st.actions || []).filter((a) => a.kind === 'phase').map((a) => `${a.sequence}`).join(', ');
    out.push(`<g class="${cls}" data-sel="s|${p}"><rect x="${cx - SW / 2}" y="${y}" width="${SW}" height="${SH}" rx="3"/>
      ${root && i === 0 ? `<rect x="${cx - SW / 2 + 4}" y="${y + 4}" width="${SW - 8}" height="${SH - 8}" rx="2" class="init"/>` : ''}
      <text x="${cx - SW / 2 + 10}" y="${y + 19}" class="n">S${p.replaceAll('/', '.')}</text><text x="${cx - SW / 2 + 22 + 8 * (p.length + 1)}" y="${y + 19}" class="nm">${esc(short(st.name, Math.max(8, 22 - p.length)))}</text>
      <text x="${cx - SW / 2 + 10}" y="${y + 38}" class="inf">${esc(phases ? short(`▶ ${phases}`, 26) : info)}${phases && info ? '' : ''}</text>
      ${act ? `<text x="${cx + SW / 2 - 8}" y="${y + 38}" class="tm" text-anchor="end">${dur(Date.now() - (o.run.since?.[p] || Date.now()))}</text>` : past?.exit ? `<text x="${cx + SW / 2 - 8}" y="${y + 38}" class="tm" text-anchor="end">${dur(past.exit - past.enter)}</text>` : ''}
      <title>${esc(st.name)}${st.description ? ` – ${esc(st.description)}` : ''}${(st.actions || []).map((a) => `\n⚡ ${esc(sub(actText(a)))}`).join('')}</title></g>`);
    y += SH;
    // rami paralleli
    if (st.branches?.length) {
      const ms = st.branches.map(measure);
      const cols = [];
      let bx = x0;
      for (const m of ms) { cols.push(bx); bx += m.w + PAD; }
      const c0 = cols[0] + SW / 2 + 10, c1 = cols.at(-1) + SW / 2 + 10;
      out.push(`<line x1="${cx}" y1="${y}" x2="${cx}" y2="${y + 10}" class="sfx-ln"/><line x1="${c0 - 12}" y1="${y + 10}" x2="${c1 + 12}" y2="${y + 10}" class="sfx-dbl"/><line x1="${c0 - 12}" y1="${y + 14}" x2="${c1 + 12}" y2="${y + 14}" class="sfx-dbl"/>`);
      const yb = y + 14;
      const ends = [];
      st.branches.forEach((b, k) => {
        const bcx = cols[k] + SW / 2 + 10;
        const bs = o.sel?.type === 'b' && o.sel.path === p && o.sel.k === k;
        out.push(`<g class="sfx-brh${bs ? ' sel' : ''}" data-sel="b|${p}|${k}"><rect x="${bcx - 34}" y="${yb + 2}" width="68" height="${BRH - 6}" rx="8"/><text x="${bcx}" y="${yb + 14}" text-anchor="middle">ramo ${k + 1}</text></g>
          <line x1="${bcx}" y1="${yb}" x2="${bcx}" y2="${yb + 2}" class="sfx-ln"/><line x1="${bcx}" y1="${yb + BRH - 4}" x2="${bcx}" y2="${yb + BRH + 4}" class="sfx-ln"/>`);
        const end = draw(b, `${p}.${k + 1}`, cols[k], yb + BRH + 6, o, out, false);
        ends.push([bcx, end]);
      });
      const yj = Math.max(...ends.map((e) => e[1])) + 10;
      for (const [bcx, e] of ends) out.push(`<line x1="${bcx}" y1="${e}" x2="${bcx}" y2="${yj}" class="sfx-ln"/>`);
      out.push(`<line x1="${c0 - 12}" y1="${yj}" x2="${c1 + 12}" y2="${yj}" class="sfx-dbl"/><line x1="${c0 - 12}" y1="${yj + 4}" x2="${c1 + 12}" y2="${yj + 4}" class="sfx-dbl"/>`);
      y = yj + 4;
    }
    // transizioni: principale (verso il passo seguente) in linea, alternative a destra con salto
    const trs = st.transitions || [];
    const mainIdx = trs.findIndex((t) => !t.next);
    const alts = trs.map((t, ti) => [t, ti]).filter(([, ti]) => ti !== mainIdx);
    const ty = y + TG / 2;
    out.push(`<line x1="${cx}" y1="${y}" x2="${cx}" y2="${ty + (mainIdx >= 0 || !trs.length ? TG / 2 : 0)}" class="sfx-ln"/>`);
    const bar = (bx, by, t, ti, jump) => {
      const ts = o.sel?.type === 't' && o.sel.path === p && o.sel.ti === ti;
      const txt = t ? trText(t) : st.branches?.length ? 'rami conclusi' : (st.actions || []).some((a) => a.kind === 'phase') ? 'fasi concluse' : 'subito';
      const tgt = jump ? ` ↷ ${jump === 'END' ? 'FINE' : jump}` : '';
      return `<g class="sfx-tr${ts ? ' sel' : ''}${t ? '' : ' impl'}" data-sel="t|${p}|${ti}"><rect x="${bx - 22}" y="${by - 10}" width="${LBL + 30}" height="20" class="hit"/>
        <line x1="${bx - 16}" y1="${by}" x2="${bx + 16}" y2="${by}" class="bar"/>
        <text x="${bx + 24}" y="${by + 4}" class="lbl">T${p.replaceAll('/', '.')}${ti >= 0 && trs.length > 1 ? `.${ti + 1}` : ''}  ${esc(short(sub(txt), 34))}<tspan class="jmp">${esc(tgt)}</tspan></text></g>`;
    };
    if (mainIdx >= 0 || !trs.length) out.push(bar(cx, ty, trs[mainIdx], mainIdx >= 0 ? mainIdx : -1, null));
    alts.forEach(([t, ti], k) => {
      const ay = ty + (mainIdx >= 0 || !trs.length ? (k + 1) * ADY : k * ADY);
      const ax = cx + 26;
      out.push(`<line x1="${cx}" y1="${y + 8}" x2="${ax}" y2="${y + 8}" class="sfx-ln alt"/><line x1="${ax}" y1="${y + 8}" x2="${ax}" y2="${ay}" class="sfx-ln alt"/>`);
      out.push(bar(ax, ay, t, ti, t.next || '(seguente)'));
    });
    y += TG + Math.max(0, alts.length - (mainIdx >= 0 || !trs.length ? 0 : 1)) * ADY;
    flows = mainIdx >= 0 || !trs.length;
    if (st.timeout_s) out.push(`<text x="${cx - SW / 2 - 4}" y="${y - TG / 2 + 4}" class="sfx-to" text-anchor="end">⏱ ${st.timeout_s}s → ${esc(st.on_timeout || 'errore')}</text>`);
    ins(y - 8, i + 1);
    void names;
  });
  if (root) {
    out.push(`<g class="sfx-end"><rect x="${cx - 36}" y="${y}" width="72" height="24" rx="12"/><text x="${cx}" y="${y + 16}" text-anchor="middle">FINE</text></g>`);
    y += 24;
  }
  return y;
}

export function sfcFlowSvg(steps, o = {}) {
  const m = measure(steps || []);
  const out = [];
  const h = draw(steps || [], '', 40, 20, o, out, true) + 20;
  const w = m.w + 80;
  const z = o.zoom || 1;
  return `<svg class="sfx-svg" width="${w * z}" height="${h * z}" viewBox="0 0 ${w} ${h}">${out.join('')}</svg>`;
}

/** Sola lettura (runtime): stato di esecuzione con passi attivi e durate. */
export function sfcFlowView(steps, run) {
  return `<div class="sfx-view">${sfcFlowSvg(steps, { run })}</div>`;
}

// ================================================================ editor interattivo
/**
 * opts: { mode: 'sequence'|'recipe', onChange, ctx: { sequences, functions, aliases(), params(), tags() } }
 */
export function sfcFlow(host, steps, opts = {}) {
  let sel = null, zoom = 1;
  const recipe = opts.mode === 'recipe';
  const changed = () => opts.onChange?.();
  const parseVal = (v) => (v === 'true' ? true : v === 'false' ? false : v.trim() !== '' && !isNaN(+v) ? +v : v);
  host.innerHTML = `<div class="sfx">
    <div class="sfx-tb">
      <button class="small" data-op="add" title="Passo + transizione dopo il passo selezionato (o in fondo)">＋ Passo</button>
      <button class="small" data-op="par" title="Rami paralleli (divergenza simultanea) nel passo selezionato">⫴ Ramo parallelo</button>
      <button class="small" data-op="alt" title="Transizione alternativa con salto (divergenza alternativa, cicli)">↷ Alternativa</button>
      <button class="small" data-op="up" title="Sposta su">⏶</button><button class="small" data-op="down" title="Sposta giù">⏷</button>
      <button class="small danger" data-op="del" title="Elimina l'elemento selezionato (Canc)">✕</button>
      <span class="grow"></span>
      <button class="small" data-z="-1" title="Zoom −">−</button><span class="muted" data-zl>100%</span><button class="small" data-z="1" title="Zoom +">+</button>
    </div>
    <div class="sfx-body"><div class="sfx-canvas" tabindex="0"></div><div class="sfx-props"></div></div>
    <datalist id="sfxTags"><option value="@elapsed">secondi nel passo</option><option value="@confirm">conferma operatore</option>${recipe ? '<option value="@phases_done">fasi del passo concluse</option>' : ''}${(opts.ctx?.tags?.() || []).map((t) => `<option value="${esc(t)}">`).join('')}</datalist>
  </div>`;
  const canvas = host.querySelector('.sfx-canvas'), props = host.querySelector('.sfx-props');

  // ------------------------------------------------------------ operazioni
  const ops = {
    add(atList, at) {
      const list = listAt(steps, atList);
      const st = { name: uniq(list, 'Passo') };
      list.splice(at, 0, st);
      sel = { type: 's', path: atList ? `${atList}/${at + 1}` : String(at + 1) };
      return true;
    },
    addAfter() {
      if (sel?.type === 's' || sel?.type === 't') { const { lp, i } = parentOf(sel.path); return ops.add(lp, i + 1); }
      if (sel?.type === 'b') { const lp = `${sel.path}.${sel.k + 1}`; return ops.add(lp, listAt(steps, lp).length); }
      return ops.add('', steps.length);
    },
    par() {
      if (sel?.type !== 's') return false;
      const { st } = stepAt(steps, sel.path);
      (st.branches ||= []).push([{ name: `R${st.branches.length + 1}.1` }]);
      sel = { type: 'b', path: sel.path, k: st.branches.length - 1 };
      return true;
    },
    alt() {
      if (sel?.type !== 's' && sel?.type !== 't') return false;
      const { list, st } = stepAt(steps, sel.path);
      const target = list[0]?.name || 'END';
      (st.transitions ||= []).push({ when: [{ tag: '', op: '==', value: true }], next: target === st.name && list.length > 1 ? list[1].name : target });
      sel = { type: 't', path: sel.path, ti: st.transitions.length - 1 };
      return true;
    },
    move(d) {
      if (sel?.type === 's') {
        const { list, i } = stepAt(steps, sel.path);
        const j = i + d;
        if (j < 0 || j >= list.length) return false;
        [list[i], list[j]] = [list[j], list[i]];
        const { lp } = parentOf(sel.path);
        sel = { type: 's', path: lp ? `${lp}/${j + 1}` : String(j + 1) };
        return true;
      }
      if (sel?.type === 't' && sel.ti >= 0) {
        const { st } = stepAt(steps, sel.path);
        const j = sel.ti + d;
        if (j < 0 || j >= st.transitions.length) return false;
        [st.transitions[sel.ti], st.transitions[j]] = [st.transitions[j], st.transitions[sel.ti]];
        sel.ti = j;
        return true;
      }
      return false;
    },
    del() {
      if (!sel) return false;
      const { list, i, st } = stepAt(steps, sel.path);
      if (sel.type === 's') {
        if (!confirm(`Eliminare il passo ${st.name}?`)) return false;
        list.splice(i, 1);
        for (const x of list) { if (x.transitions) x.transitions = x.transitions.filter((t) => t.next !== st.name); if (!x.transitions?.length) delete x.transitions; if (x.on_timeout === st.name) delete x.on_timeout; }
      } else if (sel.type === 't') {
        if (sel.ti < 0) return false;
        st.transitions.splice(sel.ti, 1);
        if (!st.transitions.length) delete st.transitions;
      } else if (sel.type === 'b') {
        if (!confirm(`Eliminare il ramo ${sel.k + 1}?`)) return false;
        st.branches.splice(sel.k, 1);
        if (!st.branches.length) delete st.branches;
      }
      sel = null;
      return true;
    },
  };
  const doOp = (fn) => { if (fn()) { render(); changed(); } };

  // ------------------------------------------------------------ disegno e selezione
  function render() {
    canvas.innerHTML = sfcFlowSvg(steps, { sel, edit: true, zoom });
    host.querySelector('[data-zl]').textContent = `${Math.round(zoom * 100)}%`;
    const has = (t) => sel?.type === t;
    host.querySelector('[data-op="par"]').disabled = !has('s');
    host.querySelector('[data-op="alt"]').disabled = !has('s') && !has('t');
    host.querySelector('[data-op="up"]').disabled = host.querySelector('[data-op="down"]').disabled = !has('s') && !(has('t') && sel.ti >= 0);
    host.querySelector('[data-op="del"]').disabled = !sel || (has('t') && sel.ti < 0);
    renderProps();
  }
  const parseSel = (el) => {
    const [type, path, x] = el.dataset.sel.split('|');
    return type === 's' ? { type, path } : type === 't' ? { type, path, ti: +x } : { type, path, k: +x };
  };
  canvas.addEventListener('click', (e) => {
    const ins = e.target.closest('[data-ins]');
    if (ins) { const [lp, idx] = ins.dataset.ins.split('|'); doOp(() => ops.add(lp, +idx)); return; }
    const g = e.target.closest('[data-sel]');
    sel = g ? parseSel(g) : null;
    render();
  });
  canvas.addEventListener('contextmenu', (e) => {
    const g = e.target.closest('[data-sel]');
    if (g) { sel = parseSel(g); render(); }
    const items = [
      { header: sel ? (sel.type === 's' ? `Passo ${stepAt(steps, sel.path).st?.name}` : sel.type === 't' ? 'Transizione' : `Ramo ${sel.k + 1}`) : 'SFC' },
      { label: 'Inserisci passo dopo', icon: '＋', action: () => doOp(ops.addAfter) },
      sel?.type === 's' ? { label: 'Inserisci passo prima', icon: '＋', action: () => { const { lp, i } = parentOf(sel.path); doOp(() => ops.add(lp, i)); } } : null,
      sel?.type === 's' ? { label: 'Aggiungi ramo parallelo', icon: '⫴', action: () => doOp(ops.par) } : null,
      sel?.type === 's' || sel?.type === 't' ? { label: 'Aggiungi transizione alternativa (salto)', icon: '↷', action: () => doOp(ops.alt) } : null,
      sel?.type === 's' && !stepAt(steps, sel.path).st?.transitions?.length ? { label: 'Aggiungi condizione di transizione', icon: '➜', action: () => doOp(() => { const { st } = stepAt(steps, sel.path); st.transitions = [{ when: [{ tag: '', op: '==', value: true }] }]; sel = { type: 't', path: sel.path, ti: 0 }; return true; }) } : null,
      { sep: true },
      sel ? { label: 'Sposta su', icon: '⏶', action: () => doOp(() => ops.move(-1)) } : null,
      sel ? { label: 'Sposta giù', icon: '⏷', action: () => doOp(() => ops.move(1)) } : null,
      sel ? { label: 'Elimina', icon: '✕', key: 'Canc', action: () => doOp(ops.del) } : null,
    ];
    showMenu(e, items);
  });
  canvas.addEventListener('keydown', (e) => {
    if (e.key === 'Delete' && sel && !e.target.closest('input,select,textarea')) { e.preventDefault(); doOp(ops.del); }
  });
  host.querySelectorAll('[data-op]').forEach((b) => (b.onclick = () => doOp({ add: ops.addAfter, par: ops.par, alt: ops.alt, up: () => ops.move(-1), down: () => ops.move(1), del: ops.del }[b.dataset.op])));
  host.querySelectorAll('[data-z]').forEach((b) => (b.onclick = () => { zoom = Math.min(2, Math.max(0.4, zoom + 0.15 * +b.dataset.z)); render(); }));

  // ------------------------------------------------------------ pannello proprietà
  const kinds = recipe
    ? [['phase', 'Avvia fase'], ['write', 'Scrivi tag'], ['script', 'Script Python'], ['log', 'Messaggio nel log']]
    : [['write', 'Scrivi tag'], ['script', 'Script Python'], ['log', 'Messaggio nel log']];
  function actHtml(a, ai) {
    const seq = opts.ctx?.sequences?.find((s) => s.name === a.sequence);
    const aliases = opts.ctx?.aliases?.() || [];
    return `<div class="sfx-act" data-ai="${ai}"><div class="row"><select data-k="kind">${kinds.map(([k, l]) => `<option value="${k}" ${k === a.kind ? 'selected' : ''}>${l}</option>`).join('')}</select><span class="grow"></span><button class="small danger" data-adel>✕</button></div>
      ${a.kind === 'write' ? `<div class="row"><input data-k="tag" list="tagNames" value="${esc(a.tag || '')}" placeholder="{unit}.Tag" style="flex:1"><span>←</span><input data-k="value" value="${esc(a.value === undefined ? 'true' : String(a.value))}" style="width:90px" placeholder="valore o {param}"></div>`
      : a.kind === 'phase' ? `<div class="row"><select data-k="sequence" style="flex:1">${(opts.ctx?.sequences || []).map((s) => `<option ${s.name === a.sequence ? 'selected' : ''}>${esc(s.name)}</option>`).join('')}</select><span>su</span>
          <select data-k="device">${seq?.device_class ? '' : '<option value="">(nessuno)</option>'}${aliases.map((x) => `<option value="{${esc(x)}}" ${a.device === `{${x}}` ? 'selected' : ''}>{${esc(x)}}</option>`).join('')}</select></div>
          ${(seq?.parameters || []).map((p) => `<div class="row"><label style="width:110px">${esc(p.name)}</label><input data-pp="${esc(p.name)}" list="sfxParams" value="${esc(a.params?.[p.name] ?? '')}" placeholder="${esc(p.default ?? '')}" style="flex:1"><span class="muted">${esc(p.unit || '')}</span></div>`).join('')}`
      : a.kind === 'script' ? `<select data-k="fn">${(opts.ctx?.functions || []).map((f) => `<option ${f === a.fn ? 'selected' : ''}>${esc(f)}</option>`).join('')}</select>`
      : `<input data-k="text" value="${esc(a.text || '')}" placeholder="testo, es. {unit} pronto" style="width:100%">`}</div>`;
  }
  function renderProps() {
    if (!sel) {
      props.innerHTML = `<h4>SFC ${recipe ? '– procedura' : '– fase'}</h4><p class="hint">Clic su un passo, una transizione o l'etichetta di un ramo per modificarli. <b>+</b> sui collegamenti inserisce un passo; tasto destro per il menu.</p>
        <ul class="hint"><li><b>Passo</b>: azioni all'ingresso${recipe ? ' (avvio delle fasi)' : ''}, tempi.</li><li><b>Transizione</b> (barra): condizione per passare al passo seguente.</li>
        <li><b>Alternativa</b> (↷): transizione verso un altro passo (salto, ciclo) o FINE; vince la prima vera.</li><li><b>Rami paralleli</b> (doppia linea): partono insieme, il passo prosegue quando tutti sono in fondo.</li></ul>
        <p class="hint">${steps.length} passi.</p>`;
      return;
    }
    const { list, st } = stepAt(steps, sel.path);
    if (!st) { sel = null; return renderProps(); }
    const names = list.map((s) => s.name);
    if (sel.type === 's') {
      props.innerHTML = `<h4>Passo S${sel.path.replaceAll('/', '.')}</h4>
        <div class="fg"><label>Nome</label><input data-sf="name" value="${esc(st.name)}">
        <label>Descrizione</label><input data-sf="description" value="${esc(st.description || '')}">
        <label title="Tempo minimo prima di valutare le transizioni">T. minimo (s)</label><input type="number" data-sf="min_time_s" value="${st.min_time_s ?? ''}">
        <label title="vuoto = nessun timeout">Timeout (s)</label><input type="number" data-sf="timeout_s" value="${st.timeout_s ?? ''}">
        <label>In timeout →</label><select data-sf="on_timeout"><option value="">errore + allarme</option>${names.map((n) => `<option ${n === st.on_timeout ? 'selected' : ''}>${esc(n)}</option>`).join('')}</select></div>
        <h5>⚡ Azioni all'ingresso</h5>${(st.actions || []).map(actHtml).join('') || '<p class="muted">nessuna</p>'}<button class="small" data-aadd>+ Azione</button>
        <datalist id="sfxParams">${(opts.ctx?.params?.() || []).map((x) => `<option value="{${esc(x)}}">`).join('')}</datalist>
        <h5>Struttura</h5><div class="row" style="flex-wrap:wrap"><button class="small" data-op2="par">⫴ Ramo parallelo</button><button class="small" data-op2="alt">↷ Alternativa</button>${!st.transitions?.length ? '<button class="small" data-op2="cond">➜ Condizione</button>' : ''}</div>`;
      props.querySelectorAll('[data-sf]').forEach((el) => (el.onchange = () => {
        const k = el.dataset.sf, old = st.name;
        if (['min_time_s', 'timeout_s'].includes(k)) { if (el.value === '' || +el.value === 0) delete st[k]; else st[k] = +el.value; }
        else if (el.value === '' && k !== 'name') delete st[k];
        else st[k] = el.value.trim();
        if (k === 'name' && old !== st.name) for (const x of list) { for (const t of x.transitions || []) if (t.next === old) t.next = st.name; if (x.on_timeout === old) x.on_timeout = st.name; }
        render(); changed();
      }));
      props.querySelector('[data-aadd]').onclick = () => { (st.actions ||= []).push(recipe ? { kind: 'phase', sequence: opts.ctx?.sequences?.[0]?.name || '', device: `{${opts.ctx?.aliases?.()[0] || ''}}` } : { kind: 'write', tag: '', value: true }); render(); changed(); };
      props.querySelectorAll('[data-ai]').forEach((box) => {
        const a = st.actions[+box.dataset.ai];
        box.querySelectorAll('[data-k]').forEach((el) => (el.onchange = () => {
          const k = el.dataset.k;
          if (k === 'kind') { for (const x of Object.keys(a)) delete a[x]; a.kind = el.value; if (a.kind === 'phase') { a.sequence = opts.ctx?.sequences?.[0]?.name || ''; a.device = `{${opts.ctx?.aliases?.()[0] || ''}}`; } }
          else if (k === 'sequence') { a.sequence = el.value; a.params = {}; }
          else if (k === 'device' && !el.value) delete a.device;
          else a[k] = k === 'value' ? parseVal(el.value) : el.value;
          render(); changed();
        }));
        box.querySelectorAll('[data-pp]').forEach((el) => (el.onchange = () => {
          a.params ||= {};
          if (el.value.trim()) a.params[el.dataset.pp] = el.value.trim(); else delete a.params[el.dataset.pp];
          if (!Object.keys(a.params).length) delete a.params;
          render(); changed();
        }));
        box.querySelector('[data-adel]').onclick = () => { st.actions.splice(+box.dataset.ai, 1); if (!st.actions.length) delete st.actions; render(); changed(); };
      });
      props.querySelectorAll('[data-op2]').forEach((b) => (b.onclick = () => doOp(b.dataset.op2 === 'par' ? ops.par : b.dataset.op2 === 'alt' ? ops.alt : () => { st.transitions = [{ when: [{ tag: '', op: '==', value: true }] }]; sel = { type: 't', path: sel.path, ti: 0 }; return true; })));
    } else if (sel.type === 't') {
      if (sel.ti < 0) {
        props.innerHTML = `<h4>Transizione T${sel.path.replaceAll('/', '.')}</h4><p class="hint">Implicita: il passo prosegue ${st.branches?.length ? 'quando tutti i rami sono conclusi' : (st.actions || []).some((a) => a.kind === 'phase') ? 'quando le fasi avviate sono concluse' : 'subito dopo le azioni'}.</p>
          <button class="small primary" data-mk>➜ Aggiungi una condizione</button>`;
        props.querySelector('[data-mk]').onclick = () => doOp(() => { st.transitions = [{ when: [{ tag: '', op: '==', value: true }] }]; sel.ti = 0; return true; });
        return;
      }
      const t = st.transitions[sel.ti];
      props.innerHTML = `<h4>Transizione T${sel.path.replaceAll('/', '.')}${st.transitions.length > 1 ? `.${sel.ti + 1}` : ''}</h4>
        <div class="fg"><label>Etichetta</label><input data-tf="label" value="${esc(t.label || '')}" placeholder="es. livello raggiunto">
        <label>Condizioni</label><select data-tf="mode"><option value="all" ${t.mode !== 'any' ? 'selected' : ''}>tutte (E)</option><option value="any" ${t.mode === 'any' ? 'selected' : ''}>almeno una (O)</option></select>
        <label>Poi vai a</label><select data-tf="next"><option value="" ${!t.next ? 'selected' : ''}>passo seguente</option>${names.map((n) => `<option ${n === t.next ? 'selected' : ''}>${esc(n)}</option>`).join('')}<option value="END" ${t.next === 'END' ? 'selected' : ''}>FINE${parentOf(sel.path).lp ? ' del ramo' : ''}</option></select></div>
        <h5>Condizioni</h5>${(t.when || []).map((c, ci) => `<div class="sfx-cond" data-ci="${ci}"><input data-cf="tag" list="sfxTags" value="${esc(c.tag)}" placeholder="tag, @elapsed, @confirm${recipe ? ', @phases_done' : ''}" style="flex:1">
          <select data-cf="op">${OPS.map((o) => `<option ${o === c.op ? 'selected' : ''}>${o}</option>`).join('')}</select>
          <input data-cf="value" value="${esc(String(c.value ?? ''))}" placeholder="valore, {param}, tag:Nome" style="width:110px"><button class="small danger" data-cdel>✕</button></div>`).join('') || '<p class="muted">nessuna: sempre vera</p>'}
        <button class="small" data-cadd>+ Condizione</button>
        <p class="hint">Le transizioni di un passo si valutano in ordine: vince la prima vera. Con "passo seguente" la transizione è sulla linea principale; le altre sono alternative (salti, cicli).</p>`;
      props.querySelectorAll('[data-tf]').forEach((el) => (el.onchange = () => { const k = el.dataset.tf; if (el.value === '' || (k === 'mode' && el.value === 'all')) delete t[k]; else t[k] = el.value; render(); changed(); }));
      props.querySelector('[data-cadd]').onclick = () => { (t.when ||= []).push({ tag: '', op: '==', value: true }); render(); changed(); };
      props.querySelectorAll('[data-ci]').forEach((cr) => {
        const c = t.when[+cr.dataset.ci];
        cr.querySelectorAll('[data-cf]').forEach((el) => (el.onchange = () => { c[el.dataset.cf] = el.dataset.cf === 'value' && !el.value.startsWith('tag:') && !el.value.includes('{') ? parseVal(el.value) : el.value.trim(); render(); changed(); }));
        cr.querySelector('[data-cdel]').onclick = () => { t.when.splice(+cr.dataset.ci, 1); if (!t.when.length) delete t.when; render(); changed(); };
      });
    } else {
      const b = st.branches[sel.k];
      props.innerHTML = `<h4>Ramo parallelo ${sel.k + 1} di ${esc(st.name)}</h4><p class="hint">${b.length} passi. Parte all'ingresso nel passo, insieme agli altri rami; il passo prosegue quando tutti i rami sono arrivati in fondo (o a FINE).</p>
        <div class="row"><button class="small" data-bb="add">＋ Passo in fondo al ramo</button><button class="small" data-bb="new">⫴ Nuovo ramo</button><button class="small danger" data-bb="del">✕ Elimina ramo</button></div>`;
      props.querySelector('[data-bb="add"]').onclick = () => doOp(() => ops.add(`${sel.path}.${sel.k + 1}`, b.length));
      props.querySelector('[data-bb="new"]').onclick = () => doOp(() => { sel = { type: 's', path: sel.path }; return ops.par(); });
      props.querySelector('[data-bb="del"]').onclick = () => doOp(ops.del);
    }
  }

  render();
  return { redraw: () => render(), select: (s) => { sel = s; render(); } };
}
