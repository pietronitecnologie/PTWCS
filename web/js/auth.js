// Login, utente corrente e permessi lato client (il controllo vero è sul server).
import { api, esc, toast } from './core.js';

export const AUTH = { me: null, viewOps: {} };
window.PTWCS_AUTH = AUTH;

export const can = (perm) => {
  const m = AUTH.me;
  if (!m || !m.enabled || m.all) return true;
  return m.permissions.some((p) => p === perm || (p.endsWith('.*') && perm.startsWith(p.slice(0, -1))));
};
export const inRoles = (roles) => !roles?.length || !AUTH.me?.enabled || AUTH.me.all || AUTH.me.roles.some((r) => roles.includes(r));
AUTH.can = can;
AUTH.inRoles = inRoles;

/** Motivo per cui un'azione di una vista non è consentita (null = consentita). */
AUTH.denyAction = (a, viewId) => {
  const need = { write: 'tag.write', script: 'script.call', wh: a.op === 'search' ? null : 'wh.operate' }[a.kind];
  if (need && !can(need)) return 'Permesso negato: comando non consentito al tuo ruolo';
  if (a.roles?.length && !inRoles(a.roles)) return `Comando riservato ai ruoli: ${a.roles.join(', ')}`;
  if (need && viewId && AUTH.viewOps[viewId] === false) return 'I comandi di questa vista non sono consentiti al tuo ruolo';
  return null;
};

function overlay(html) {
  document.getElementById('authOverlay')?.remove();
  const o = document.createElement('div');
  o.id = 'authOverlay';
  o.className = 'auth-overlay';
  o.innerHTML = `<div class="auth-box">${html}</div>`;
  document.body.appendChild(o);
  return o;
}

function plantName() { return document.title || 'PTWCS'; }

/** Schermata di login (blocca la pagina finché il login non riesce). */
export function showLogin(message = '') {
  return new Promise(() => {
    const o = overlay(`<div class="auth-logo">🔐</div><h2>${esc(plantName())}</h2>
      <p class="muted">${esc(message || 'Accedi per continuare')}</p>
      <form id="lf"><input id="lu" placeholder="Utente" autocomplete="username" autofocus>
      <input id="lp" type="password" placeholder="Password" autocomplete="current-password">
      <div class="err" id="le"></div><button class="primary" type="submit">Accedi</button></form>
      ${AUTH.me?.anonymous_role ? '<button class="small" id="lAnon" style="margin-top:8px">Continua senza login (' + esc(AUTH.me.anonymous_role) + ')</button>' : ''}`);
    o.querySelector('#lu').focus();
    o.querySelector('#lAnon')?.addEventListener('click', () => location.reload());
    o.querySelector('#lf').onsubmit = async (e) => {
      e.preventDefault();
      try {
        const me = await api.post('/api/auth/login', { username: o.querySelector('#lu').value, password: o.querySelector('#lp').value });
        AUTH.me = me;
        location.reload();
      } catch (err) {
        o.querySelector('#le').textContent = err.message;
        o.querySelector('#lp').select();
      }
    };
  });
}

/** Cambio password (obbligatorio se `forced`). */
export function showChangePassword(forced = false) {
  return new Promise((resolve) => {
    const o = overlay(`<div class="auth-logo">🔑</div><h2>Cambio password</h2>
      <p class="muted">${forced ? 'Al primo accesso è obbligatorio scegliere una nuova password.' : `Utente ${esc(AUTH.me?.user || '')}`}</p>
      <form id="pf"><input id="po" type="password" placeholder="Password attuale" autocomplete="current-password">
      <input id="pn" type="password" placeholder="Nuova password (min. 6 caratteri)" autocomplete="new-password">
      <input id="pc" type="password" placeholder="Conferma nuova password" autocomplete="new-password">
      <div class="err" id="pe"></div><button class="primary" type="submit">Cambia password</button></form>
      ${forced ? '<button class="small" id="pOut" style="margin-top:8px">Esci</button>' : '<button class="small" id="pX" style="margin-top:8px">Annulla</button>'}`);
    o.querySelector('#po').focus();
    o.querySelector('#pOut')?.addEventListener('click', logout);
    o.querySelector('#pX')?.addEventListener('click', () => { o.remove(); resolve(false); });
    o.querySelector('#pf').onsubmit = async (e) => {
      e.preventDefault();
      const [old, n, c] = ['#po', '#pn', '#pc'].map((s) => o.querySelector(s).value);
      if (n !== c) { o.querySelector('#pe').textContent = 'Le due password non coincidono'; return; }
      try {
        AUTH.me = await api.post('/api/auth/password', { old, new: n });
        toast('Password cambiata', 'ok');
        if (forced) location.reload(); else { o.remove(); resolve(true); }
      } catch (err) { o.querySelector('#pe').textContent = err.message; }
    };
  });
}

export async function logout() {
  try { await api.post('/api/auth/logout'); } catch { /* già uscito */ }
  location.reload();
}

/**
 * Da chiamare all'avvio: verifica sessione e permesso minimo della pagina.
 * Ritorna l'utente corrente (o resta sulla schermata di login).
 */
export async function initAuth(perm) {
  AUTH.me = await api.get('/api/auth/me');
  const m = AUTH.me;
  if (!m.enabled) return m;
  if (m.must_change) await showChangePassword(true);
  if (!can(perm)) {
    if (!m.user) await showLogin();
    overlay(`<div class="auth-logo">⛔</div><h2>Accesso negato</h2><p class="muted">L'utente <b>${esc(m.user)}</b> (${esc(m.roles.join(', ') || 'nessun ruolo')}) non ha il permesso <code>${esc(perm)}</code>.</p>
      <button class="primary" id="aOut">Cambia utente</button>`).querySelector('#aOut').onclick = logout;
    await new Promise(() => {});
  }
  applyPerms(document.body);
  new MutationObserver((ms) => ms.forEach((x) => x.addedNodes.forEach((n) => n.nodeType === 1 && applyPerms(n)))).observe(document.body, { childList: true, subtree: true });
  return m;
}

/** Disabilita gli elementi `data-perm="..."` se manca il permesso. */
export function applyPerms(root) {
  if (!AUTH.me?.enabled) return;
  const els = root.matches?.('[data-perm]') ? [root] : [];
  els.push(...(root.querySelectorAll?.('[data-perm]') || []));
  for (const el of els) {
    if (can(el.dataset.perm)) continue;
    el.disabled = true;
    el.classList.add('noperm');
    el.title = `Permesso mancante: ${el.dataset.perm}`;
  }
}

/** Widget utente nell'intestazione: nome e menu (cambio password, esci) o "Accedi". */
export function mountUserChip(el) {
  const m = AUTH.me;
  if (!el || !m?.enabled) { el?.remove(); return; }
  if (!m.user) {
    el.innerHTML = `<button class="small" id="ucLogin">🔐 Accedi</button>`;
    el.querySelector('#ucLogin').onclick = () => showLogin();
    return;
  }
  el.innerHTML = `<button class="small userchip" title="${esc(m.full_name || m.user)}\nRuoli: ${esc(m.roles.join(', '))}">👤 ${esc(m.user)}${el.dataset.compact ? '' : ` <span class="muted">${esc(m.roles[0] || '')}</span>`} ▾</button>`;
  el.querySelector('button').onclick = async (e) => {
    const { showMenu } = await import('./ctxmenu.js');
    const r = e.currentTarget.getBoundingClientRect();
    showMenu({ preventDefault() {}, stopPropagation() {}, clientX: r.left, clientY: r.bottom + 4 }, [
      { header: `${m.full_name || m.user} · ${m.roles.join(', ')}` },
      { label: 'Cambia password', icon: '🔑', action: () => showChangePassword(false) },
      { label: 'Cambia utente', icon: '👥', action: async () => { try { await api.post('/api/auth/logout'); } catch { /* */ } showLogin('Accedi con un altro utente'); } },
      { label: 'Esci', icon: '⏻', action: logout },
    ]);
  };
}

// errori di autenticazione da qualsiasi chiamata API
window.addEventListener('ptwcs-auth', (e) => {
  if (document.getElementById('authOverlay')) return;
  if (e.detail?.must_change) showChangePassword(true);
  else if (e.detail?.login) showLogin('Sessione scaduta: accedi di nuovo');
});
