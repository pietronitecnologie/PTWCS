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
