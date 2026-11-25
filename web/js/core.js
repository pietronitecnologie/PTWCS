// Client comune: API REST + connessione live (WebSocket) ai tag e agli eventi.

async function req(method, url, body) {
  const r = await fetch(url, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : {},
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const text = await r.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = text; }
  if (!r.ok) throw new Error((data && data.error) || `${r.status} ${r.statusText}`);
  return data;
}

export const api = {
  get: (u) => req('GET', u),
  put: (u, b) => req('PUT', u, b),
  post: (u, b) => req('POST', u, b ?? {}),
  del: (u) => req('DELETE', u),
};

export function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

export function fmtTs(ts) {
  if (!ts) return '';
  const d = new Date(ts);
  return d.toLocaleDateString() + ' ' + d.toLocaleTimeString();
}

/** Connessione live: mantiene l'immagine dei tag e notifica i listener. */
export class Live {
  constructor() {
    this.tags = new Map();      // nome -> {value, quality, ts}
    this.alarms = [];
    this.status = null;
    this.connected = false;
    this.handlers = {};
    this._delay = 500;
    this.hist = new Map();       // nome -> [[ts, valore numerico]] (buffer trend)
    this.histLoaded = new Set();
  }

  /** Serie per i trend (storico dal server + aggiornamenti live). */
  history(name) { return this.hist.get(name) || []; }

  _push(name, st) {
    const v = typeof st.value === 'boolean' ? (st.value ? 1 : 0) : st.value;
    if (typeof v !== 'number' || !isFinite(v)) return;
    let a = this.hist.get(name);
    if (!a) { a = []; this.hist.set(name, a); }
    if (a.length && a.at(-1)[0] >= st.ts) return;
    a.push([st.ts, v]);
    if (a.length > 4000) a.splice(0, a.length - 4000);
  }

  /** Carica (una volta) lo storico di 1 ora dei tag richiesti da un trend. */
  watchHistory(names) {
    for (const n of names) {
      if (this.histLoaded.has(n)) continue;
      this.histLoaded.add(n);
      const to = Date.now(), from = to - 3600_000;
      api.get(`/api/trend?tag=${encodeURIComponent(n)}&from=${from}&to=${to}`).then((rows) => {
        const pts = rows.filter((r) => r[1] !== null);
        const cur = this.hist.get(n) || [];
        const first = cur.length ? cur[0][0] : Infinity;
        this.hist.set(n, [...pts.filter((p) => p[0] < first), ...cur]);
        this.emit('tags', [n]);
      }).catch(() => {});
    }
  }
  on(type, fn) { (this.handlers[type] ||= []).push(fn); return this; }
  emit(type, data) { (this.handlers[type] || []).forEach((f) => { try { f(data); } catch (e) { console.error(e); } }); }
  value(name) { return this.tags.get(name)?.value; }

  connect() {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${location.host}/ws`);
    ws.onopen = () => { this.connected = true; this._delay = 500; this.emit('connection', true); };
    ws.onclose = () => {
      this.connected = false;
      this.emit('connection', false);
      setTimeout(() => this.connect(), this._delay);
      this._delay = Math.min(this._delay * 2, 8000);
    };
    ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      switch (msg.type) {
        case 'snapshot':
          this.tags = new Map(Object.entries(msg.tags));
          for (const [k, v] of this.tags) this._push(k, v);
          this.alarms = msg.alarms;
          this.emit('tags', [...this.tags.keys()]);
          this.emit('alarms', this.alarms);
          break;
        case 'tags':
          for (const [k, v] of Object.entries(msg.tags)) { this.tags.set(k, v); this._push(k, v); }
          this.emit('tags', Object.keys(msg.tags));
          break;
        case 'alarms': this.alarms = msg.data; this.emit('alarms', msg.data); break;
        case 'status': this.status = msg.data; this.emit('status', msg.data); break;
        default: this.emit(msg.type, msg.data);
      }
    };
  }
}

/** Piccola notifica a scomparsa. */
export function toast(msg, kind = 'info') {
  let box = document.getElementById('toasts');
  if (!box) { box = document.createElement('div'); box.id = 'toasts'; document.body.appendChild(box); }
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = msg;
  box.appendChild(el);
  setTimeout(() => el.classList.add('out'), 3500);
  setTimeout(() => el.remove(), 4000);
}
