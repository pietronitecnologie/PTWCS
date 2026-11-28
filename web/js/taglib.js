// Libreria dei tag in formato testo (YAML): esportazione e importazione di tipi UDT,
// istanze e tag, verso un file o tramite copia/incolla.
import { api, esc, toast } from './core.js';
import { modal } from './forms.js';

const fileName = (s) => (s || 'libreria').replace(/[^\w.-]+/g, '_');

/** Scarica un testo come file (nel client desktop finisce nella cartella Download). */
export function saveText(name, text) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type: 'text/yaml' }));
  a.download = name;
  document.body.appendChild(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
}

/**
 * Esporta in testo. `src`: { title, name, types, instances?, tags? }.
 * Si scelgono i tipi UDT (tutti selezionati) e se includere istanze e tag.
 */
export function exportDialog(src) {
  const { types = [], instances = [], tags = [] } = src;
  const m = modal(src.title || 'Esporta libreria tag', `
    <div class="field"><label>Nome della libreria</label><input id="xlName" value="${esc(src.name || '')}"></div>
    <div class="row"><b class="grow">Tipi UDT (${types.length})</b><button class="small" id="xlAll">tutti</button><button class="small" id="xlNone">nessuno</button></div>
    <div class="xl-list">${types.map((t) => `<label class="row"><input type="checkbox" data-ty="${esc(t.name)}" ${src.only && !src.only.includes(t.name) ? '' : 'checked'}> <b>${esc(t.name)}</b> <span class="muted">${esc(t.description || '')} · ${t.members.length} membri</span></label>`).join('') || '<p class="muted">nessun tipo</p>'}</div>
    ${instances.length ? `<label class="row"><input type="checkbox" id="xlInst" ${src.all ? 'checked' : ''}> Istanze UDT dei tipi scelti (${instances.length} in totale)</label>` : ''}
    ${tags.length ? `<label class="row"><input type="checkbox" id="xlTags" ${src.all ? 'checked' : ''}> Tag di progetto (${tags.length})</label>` : ''}
    <p class="hint">Formato testo YAML: si può leggere e modificare con un editor e reimportare in un altro progetto o nella libreria standard.</p>
    <textarea id="xlText" class="mono" rows="10" style="width:100%;display:none" readonly></textarea>`,
  [['Chiudi', '', null], ['📋 Mostra testo', '', async (b) => { const t = await build(b); if (t) { const ta = b.querySelector('#xlText'); ta.style.display = ''; ta.value = t; ta.select(); } return false; }],
    ['⬆ Salva file', 'primary', async (b) => {
      const t = await build(b);
      if (!t) return false;
      saveText(`${fileName(b.querySelector('#xlName').value)}.ptlib.yaml`, t);
      toast('Libreria esportata', 'ok');
    }]], { wide: true });
  const all = (v) => m.querySelectorAll('[data-ty]').forEach((c) => (c.checked = v));
  m.querySelector('#xlAll').onclick = () => all(true);
  m.querySelector('#xlNone').onclick = () => all(false);
  async function build(b) {
    const sel = new Set([...b.querySelectorAll('[data-ty]:checked')].map((c) => c.dataset.ty));
    const lib = { name: b.querySelector('#xlName').value.trim(), udt_types: types.filter((t) => sel.has(t.name)) };
    if (b.querySelector('#xlInst')?.checked) lib.udt_instances = instances.filter((i) => sel.has(i.type));
    if (b.querySelector('#xlTags')?.checked) lib.tags = tags;
    if (!lib.udt_types.length && !lib.udt_instances?.length && !lib.tags?.length) { toast('Niente da esportare', 'error'); return null; }
    try { return (await api.post('/api/taglib/text', lib)).text; } catch (e) { toast(e.message, 'error'); return null; }
  }
}

/**
 * Importa da file o testo. `dst`: { title, types, instances?, tags?, onApply(result) }.
 * Con `instances`/`tags` assenti si importano solo i tipi UDT (libreria standard).
 * I nomi già presenti si sostituiscono, si saltano o si rinominano.
 */
export function importDialog(dst) {
  let lib = null;
  const m = modal(dst.title || 'Importa libreria tag', `
    <div class="row"><input type="file" id="ilFile" accept=".yaml,.yml,.json,.txt,.ptlib"><span class="muted">oppure incolla il testo</span></div>
    <textarea id="ilText" class="mono" rows="8" style="width:100%" placeholder="format: ptwcs-tag-library&#10;udt_types:&#10;  - name: …"></textarea>
    <div class="row"><button class="small" id="ilParse">🔍 Analizza</button><span class="grow"></span>
      <label>Nomi già presenti <select id="ilMode"><option value="replace">sostituisci</option><option value="skip">salta</option><option value="rename">rinomina (_2, _3…)</option></select></label></div>
    <div id="ilPrev"></div>`,
  [['Annulla', '', null], ['⬇ Importa', 'primary', () => {
    if (!lib) { toast('Analizza prima il testo', 'error'); return false; }
    const r = merge(lib, m.querySelector('#ilMode').value);
    dst.onApply(r);
    toast(`Importati: ${r.added.types} tipi nuovi, ${r.replaced.types} sostituiti${r.added.inst + r.replaced.inst ? `, ${r.added.inst + r.replaced.inst} istanze` : ''}${r.added.tags + r.replaced.tags ? `, ${r.added.tags + r.replaced.tags} tag` : ''}`, 'ok');
  }]], { wide: true });
  const prev = m.querySelector('#ilPrev');
  m.querySelector('#ilFile').onchange = async (e) => { const f = e.target.files[0]; if (f) { m.querySelector('#ilText').value = await f.text(); parse(); } };
  m.querySelector('#ilParse').onclick = parse;
  async function parse() {
    lib = null;
    try {
      lib = await api.post('/api/taglib/parse', { text: m.querySelector('#ilText').value });
    } catch (e) { prev.innerHTML = `<p class="chip err">✕ ${esc(e.message)}</p>`; return; }
    const has = (list, n) => list.some((x) => x.name === n);
    const row = (n, ex, extra = '') => `<li>${ex ? '<span class="chip warn">esiste</span>' : '<span class="chip ok">nuovo</span>'} <b>${esc(n)}</b> <span class="muted">${extra}</span></li>`;
    const ignored = !dst.instances && (lib.udt_instances?.length || lib.tags?.length);
    prev.innerHTML = `<p>✓ ${lib.name ? `<b>${esc(lib.name)}</b> · ` : ''}${lib.exported ? `esportata ${esc(lib.exported)} · ` : ''}${(lib.udt_types || []).length} tipi UDT, ${(lib.udt_instances || []).length} istanze, ${(lib.tags || []).length} tag</p>
      <ul class="il-list">${(lib.udt_types || []).map((t) => row(t.name, has(dst.types, t.name), `tipo · ${t.members.length} membri`)).join('')}
      ${dst.instances ? (lib.udt_instances || []).map((i) => row(i.name, has(dst.instances, i.name), `istanza di ${esc(i.type)} su ${esc(i.driver)}`)).join('') : ''}
      ${dst.tags ? (lib.tags || []).map((t) => row(t.name, has(dst.tags, t.name), `tag ${esc(t.type)} · ${esc(t.driver)} ${esc(t.address)}`)).join('') : ''}</ul>
      ${ignored ? '<p class="hint">La libreria standard contiene solo tipi UDT: istanze e tag del file vengono ignorati.</p>' : ''}`;
  }
  function merge(src, mode) {
    const out = { added: { types: 0, inst: 0, tags: 0 }, replaced: { types: 0, inst: 0, tags: 0 }, renamed: {}, first: null };
    const put = (list, items, key, onRename) => {
      for (const it of items || []) {
        const x = structuredClone(it);
        const i = list.findIndex((y) => y.name === x.name);
        if (i < 0) { list.push(x); out.added[key]++; }
        else if (mode === 'replace') { list[i] = x; out.replaced[key]++; }
        else if (mode === 'rename') {
          let k = 2;
          while (list.some((y) => y.name === `${x.name}_${k}`)) k++;
          onRename?.(x.name, `${x.name}_${k}`);
          x.name = `${x.name}_${k}`; list.push(x); out.added[key]++;
        }
        if (key === 'types') out.first ||= x.name;
      }
    };
    const ren = {};
    put(dst.types, src.udt_types, 'types', (a, b) => (ren[a] = b));
    if (dst.instances) put(dst.instances, (src.udt_instances || []).map((i) => (ren[i.type] ? { ...i, type: ren[i.type] } : i)), 'inst');
    if (dst.tags) put(dst.tags, src.tags, 'tags');
    out.renamed = ren;
    return out;
  }
}
