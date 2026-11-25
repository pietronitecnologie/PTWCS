// Utilità per i form del Designer: modale, editor tabellare, campi.
import { esc } from './core.js';

/** Finestra modale. buttons: [[etichetta, classe, fn(back) -> false per non chiudere]] */
export function modal(title, body, buttons, { wide = false } = {}) {
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
  back.close = close;
  return back;
}

const asList = (v) => (Array.isArray(v) ? v.join(', ') : v ?? '');
const fromList = (s) => s.split(',').map((x) => x.trim()).filter(Boolean);

function cellInput(col, val) {
  const v = val ?? '';
  switch (col.type) {
    case 'bool': return `<input type="checkbox" data-k="${col.key}" ${val ? 'checked' : ''}>`;
    case 'number': return `<input type="number" step="any" data-k="${col.key}" value="${esc(v)}" style="width:${col.width || 80}px">`;
    case 'select': {
      const opts = typeof col.options === 'function' ? col.options() : col.options;
      const has = opts.some((o) => (o.value ?? o) === v);
      return `<select data-k="${col.key}">${col.empty !== false ? '<option value=""></option>' : ''}${!has && v ? `<option selected>${esc(v)}</option>` : ''}${opts.map((o) => {
        const ov = o.value ?? o, ol = o.label ?? o;
        return `<option value="${esc(ov)}" ${ov === v ? 'selected' : ''}>${esc(ol)}</option>`;
      }).join('')}</select>`;
    }
    case 'list': return `<input data-k="${col.key}" value="${esc(asList(val))}" placeholder="${esc(col.placeholder || 'a, b, c')}" style="width:${col.width || 160}px">`;
    case 'live': return `<span class="mono live-val" data-live-tag="${esc(col.tag?.(arguments[2]) ?? '')}"></span>`;
    case 'button': return `<button class="small" data-btn="${col.key}" title="${esc(col.title || '')}">${col.label}${col.badge ? col.badge(arguments[2]) : ''}</button>`;
    default: return `<input data-k="${col.key}" value="${esc(typeof v === 'object' ? JSON.stringify(v) : v)}" ${col.list ? `list="${col.list}"` : ''} placeholder="${esc(col.placeholder || '')}" style="width:${col.width || 140}px" class="${col.mono ? 'mono' : ''}">`;
  }
}

/**
 * Editor tabellare di un array di oggetti (modifica in place).
 * cols: [{key, label, type: text|number|bool|select|list|live|button, options, width, onClick}]
 */
export function tableEditor(el, rows, cols, { onChange = () => {}, newRow = () => ({}), addLabel = '+ Aggiungi', readonly = () => false, reorder = true } = {}) {
  const draw = () => {
    el.innerHTML = `<div class="tbl-wrap"><table class="edit"><thead><tr>${cols.map((c) => `<th>${esc(c.label)}</th>`).join('')}<th></th></tr></thead><tbody>
      ${rows.map((r, i) => `<tr data-i="${i}" class="${readonly(r) ? 'ro' : ''}">${cols.map((c) => `<td>${cellInput(c, r[c.key], r)}</td>`).join('')}
        <td class="nowrap">${readonly(r) ? '<span class="muted" title="Non modificabile da qui">🔒</span>' : `${reorder ? `<button class="small" data-up title="Su">↑</button>` : ''}<button class="small danger" data-del title="Elimina">✕</button>`}</td></tr>`).join('')}
      </tbody></table></div>${addLabel ? `<button class="small" data-add>${esc(addLabel)}</button>` : ''}`;
    el.querySelectorAll('tr[data-i]').forEach((tr) => {
      const i = +tr.dataset.i, r = rows[i];
      const ro = readonly(r);
      tr.querySelectorAll('[data-k]').forEach((inp) => {
        const col = cols.find((c) => c.key === inp.dataset.k);
        if (ro) { inp.disabled = true; return; }
        inp.onchange = () => {
          let v;
          if (col.type === 'bool') v = inp.checked;
          else if (col.type === 'number') v = inp.value === '' ? undefined : +inp.value;
          else if (col.type === 'list') v = fromList(inp.value);
          else v = inp.value;
          if (v === '' || v === undefined || (Array.isArray(v) && !v.length)) {
            if (col.required) { inp.classList.add('invalid'); return; }
            delete r[col.key];
          } else r[col.key] = v;
          inp.classList.remove('invalid');
          onChange(r, col.key);
        };
      });
      tr.querySelectorAll('[data-btn]').forEach((b) => (b.onclick = () => cols.find((c) => c.key === b.dataset.btn).onClick(r, i, draw)));
      tr.querySelector('[data-del]')?.addEventListener('click', () => { rows.splice(i, 1); onChange(null); draw(); });
      tr.querySelector('[data-up]')?.addEventListener('click', () => { if (i > 0) { [rows[i - 1], rows[i]] = [rows[i], rows[i - 1]]; onChange(null); draw(); } });
    });
    el.querySelector('[data-add]')?.addEventListener('click', () => { rows.push(newRow()); onChange(rows.at(-1)); draw(); el.querySelector('tr:last-child input')?.focus(); });
  };
  draw();
  return { redraw: draw };
}

/** Editor chiave → valore (oggetto) come righe. */
export function kvEditor(el, obj, { onChange = () => {}, keyLabel = 'Nome', valLabel = 'Valore', valList, keyPlaceholder = '' } = {}) {
  const rows = Object.entries(obj).map(([k, v]) => ({ k, v: typeof v === 'object' ? JSON.stringify(v) : String(v ?? '') }));
  const sync = () => {
    for (const k of Object.keys(obj)) delete obj[k];
    for (const r of rows) if (r.k) obj[r.k] = r.v ?? '';
    onChange(obj);
  };
  return tableEditor(el, rows, [
    { key: 'k', label: keyLabel, width: 110, placeholder: keyPlaceholder },
    { key: 'v', label: valLabel, width: 150, list: valList },
  ], { onChange: sync, newRow: () => ({ k: '', v: '' }), addLabel: '+', reorder: false });
}


/**
 * Rende trascinabile un separatore che regola una variabile CSS di larghezza
 * sul contenitore (es. --lw). `dir` = 1 se il pannello è a sinistra del
 * separatore, -1 se è a destra. La larghezza è salvata in localStorage.
 */
export function makeSplitter(handle, container, cssVar, { dir = 1, min = 160, max = 700, def = 260, key } = {}) {
  const storeKey = key || `split${cssVar}`;
  const apply = (w) => container.style.setProperty(cssVar, `${Math.round(Math.max(min, Math.min(max, w)))}px`);
  try { const v = +localStorage.getItem(storeKey); if (v) apply(v); } catch { /* storage non disponibile */ }
  handle.addEventListener('mousedown', (e) => {
    e.preventDefault();
    const start = e.clientX;
    const cur = parseFloat(getComputedStyle(container).getPropertyValue(cssVar)) || def;
    handle.classList.add('drag');
    document.body.classList.add('resizing');
    const mv = (ev) => apply(cur + (ev.clientX - start) * dir);
    const up = () => {
      window.removeEventListener('mousemove', mv);
      window.removeEventListener('mouseup', up);
      handle.classList.remove('drag');
      document.body.classList.remove('resizing');
      try { localStorage.setItem(storeKey, parseFloat(container.style.getPropertyValue(cssVar))); } catch { /* ignore */ }
      window.dispatchEvent(new Event('resize'));
    };
    window.addEventListener('mousemove', mv);
    window.addEventListener('mouseup', up);
  });
  handle.addEventListener('dblclick', () => {
    container.style.removeProperty(cssVar);
    try { localStorage.removeItem(storeKey); } catch { /* ignore */ }
    window.dispatchEvent(new Event('resize'));
  });
}
