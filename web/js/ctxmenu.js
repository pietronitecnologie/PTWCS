// Menu contestuale (tasto destro) condiviso da Designer e runtime.
// Voce: { label, icon, key, action, disabled, checked, danger, sub: [voci] } | { sep: true } | { header: 'testo' }
import { esc } from './core.js';

let root = null;

export function closeMenu() {
  root?.remove();
  root = null;
}

function build(items, x, y, parent) {
  const el = document.createElement('div');
  el.className = 'ctxmenu';
  el.innerHTML = items.map((it, i) => {
    if (it.sep) return '<div class="cm-sep"></div>';
    if (it.header) return `<div class="cm-head">${esc(it.header)}</div>`;
    return `<div class="cm-item ${it.disabled ? 'dis' : ''} ${it.danger ? 'danger' : ''} ${it.sub ? 'has-sub' : ''}" data-i="${i}">
      <span class="cm-ic">${it.checked ? '✓' : it.icon || ''}</span><span class="cm-l">${esc(it.label)}</span>
      <span class="cm-k">${it.sub ? '▸' : esc(it.key || '')}</span></div>`;
  }).join('');
  (parent || document.body).appendChild(el);
  // posizionamento dentro la finestra
  const r = el.getBoundingClientRect();
  const W = window.innerWidth, H = window.innerHeight;
  el.style.left = `${Math.max(4, x + r.width > W ? (parent ? x - r.width - (parent.getBoundingClientRect().width) : W - r.width - 4) : x)}px`;
  el.style.top = `${Math.max(4, Math.min(y, H - r.height - 4))}px`;
  let open = null;
  el.querySelectorAll('.cm-item').forEach((row) => {
    const it = items[+row.dataset.i];
    row.onmouseenter = () => {
      open?.remove();
      open = null;
      if (it.sub && !it.disabled) {
        const rr = row.getBoundingClientRect();
        open = build(it.sub, rr.right - 2, rr.top - 5, null);
        open.classList.add('cm-sub');
        el.appendChild(open);
        // riposiziona rispetto alla finestra
        const sr = open.getBoundingClientRect();
        if (sr.right > W) open.style.left = `${rr.left - sr.width + 2}px`;
      }
    };
    row.onclick = (e) => {
      e.stopPropagation();
      if (it.disabled || it.sub) return;
      closeMenu();
      it.action?.();
    };
  });
  return el;
}

/** Mostra il menu alla posizione del mouse. Ritorna false se non ci sono voci. */
export function showMenu(e, items) {
  e.preventDefault();
  e.stopPropagation();
  closeMenu();
  const list = items.filter(Boolean);
  // rimuove separatori iniziali, finali e doppi
  const clean = list.filter((it, i) => !(it.sep && (i === 0 || i === list.length - 1 || list[i - 1]?.sep)));
  if (!clean.length) return false;
  root = build(clean, e.clientX, e.clientY);
  return true;
}

window.addEventListener('mousedown', (e) => { if (root && !e.target.closest('.ctxmenu')) closeMenu(); }, true);
window.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeMenu(); });
window.addEventListener('blur', closeMenu);
window.addEventListener('hashchange', closeMenu);

// ---------------------------------------------------------------- voci comuni (testo, appunti, link)
export async function clipWrite(text) {
  try { await navigator.clipboard.writeText(text); return true; } catch {
    const t = document.createElement('textarea');
    t.value = text; t.style.cssText = 'position:fixed;opacity:0';
    document.body.appendChild(t); t.select();
    const ok = document.execCommand('copy'); t.remove(); return ok;
  }
}
export async function clipRead() {
  try { return await navigator.clipboard.readText(); } catch { return null; }
}

/** Voci di modifica del testo per il punto cliccato (campo di input, selezione, link, cella di tabella). */
export function editItems(e, { notify } = {}) {
  const t = e.target;
  const field = t.closest('input:not([type=checkbox]):not([type=radio]):not([type=color]):not([type=range]), textarea, [contenteditable=true]');
  const sel = String(window.getSelection?.() || '');
  const items = [];
  if (field && !field.matches('[contenteditable=true]')) {
    const ro = field.readOnly || field.disabled;
    const has = field.selectionStart !== field.selectionEnd;
    const selected = () => field.value.slice(field.selectionStart, field.selectionEnd);
    const put = (txt) => { field.focus(); if (!document.execCommand('insertText', false, txt)) field.setRangeText(txt, field.selectionStart, field.selectionEnd, 'end'); field.dispatchEvent(new Event('input', { bubbles: true })); field.dispatchEvent(new Event('change', { bubbles: true })); };
    items.push(
      { label: 'Taglia', icon: '✂', key: 'Ctrl+X', disabled: ro || !has, action: async () => { await clipWrite(selected()); put(''); } },
      { label: 'Copia', icon: '⧉', key: 'Ctrl+C', disabled: !has, action: () => clipWrite(selected()) },
      { label: 'Incolla', icon: '📋', key: 'Ctrl+V', disabled: ro, action: async () => { const v = await clipRead(); if (v == null) notify?.('Incolla non consentito dal browser: usa Ctrl+V', 'error'); else put(v); } },
      { label: 'Elimina', icon: '⌫', disabled: ro || !has, action: () => put('') },
      { label: 'Seleziona tutto', icon: '▦', key: 'Ctrl+A', action: () => { field.focus(); field.select(); } },
      ...(field.value && !ro ? [{ label: 'Svuota campo', icon: '✕', action: () => { field.focus(); field.select(); put(''); } }] : []),
      { sep: true });
  } else if (sel.trim()) {
    items.push({ label: 'Copia', icon: '⧉', key: 'Ctrl+C', action: () => clipWrite(sel) }, { sep: true });
  }
  const a = t.closest('a[href]');
  if (a) {
    items.push(
      { label: 'Apri link in una nuova scheda', icon: '↗', action: () => window.open(a.href, '_blank') },
      { label: 'Copia indirizzo del link', icon: '🔗', action: () => clipWrite(a.href) }, { sep: true });
  }
  const cell = !field && t.closest('td, th, .mono, code, pre');
  if (cell && cell.textContent.trim() && !sel.trim()) {
    items.push({ label: 'Copia testo', icon: '⧉', action: () => clipWrite(cell.textContent.trim()) });
    const row = cell.closest('tr');
    if (row) items.push({ label: 'Copia riga', icon: '☰', action: () => clipWrite([...row.cells].map((c) => c.textContent.trim()).join('\t')) });
    items.push({ sep: true });
  }
  return items;
}

/**
 * Sostituisce il menu del browser ovunque non ci sia un menu specifico.
 * `extra(e)` aggiunge le voci della pagina (navigazione, aggiorna…).
 */
export function installDefaultMenu(extra = () => []) {
  document.addEventListener('contextmenu', (e) => {
    if (e.defaultPrevented || e.shiftKey) return; // Maiusc + tasto destro = menu del browser
    showMenu(e, [...editItems(e), ...extra(e)]);
  });
}
