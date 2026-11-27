// Tema chiaro / scuro (preferenza salvata nel browser; predefinito: tema del sistema).
export const theme = () => document.documentElement.dataset.theme || 'dark';

export function setTheme(t) {
  document.documentElement.dataset.theme = t;
  try { localStorage.setItem('ptwcs.theme', t); } catch { /* storage non disponibile */ }
  document.querySelectorAll('.themebtn').forEach((b) => (b.textContent = t === 'light' ? '🌙' : '☀'));
  window.dispatchEvent(new CustomEvent('ptwcs-theme', { detail: t }));
}

export const toggleTheme = () => setTheme(theme() === 'light' ? 'dark' : 'light');

export function mountThemeButton(el) {
  if (!el) return;
  el.classList.add('themebtn');
  el.title = 'Tema chiaro / scuro';
  el.textContent = theme() === 'light' ? '🌙' : '☀';
  el.onclick = toggleTheme;
}

/** Voce di menu contestuale. */
export const themeMenuItem = () => ({ label: theme() === 'light' ? 'Tema scuro' : 'Tema chiaro', icon: theme() === 'light' ? '🌙' : '☀', action: toggleTheme });
