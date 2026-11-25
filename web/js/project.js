// Stato del progetto condiviso tra i workspace del Designer.
import { api, toast } from './core.js';

export const P = { data: null, drivers: [], functions: [], dirty: false, handlers: [] };

export function onProject(fn) { P.handlers.push(fn); }
function emit() { P.handlers.forEach((f) => f(P)); }

export async function loadProject() {
  const r = await api.get('/api/project');
  P.data = r.project;
  for (const k of ['areas', 'scan_classes', 'udt_types', 'udt_instances', 'tags', 'decision_points', 'sort_classes', 'alarms', 'timer_scripts']) P.data[k] ||= [];
  P.drivers = r.drivers;
  P.functions = r.functions || [];
  P.dirty = false;
  emit();
}

export function markDirty() { P.dirty = true; emit(); }

export async function checkProject() { return api.post('/api/project/check', P.data); }

export async function saveProject() {
  try {
    const r = await api.put('/api/project', P.data);
    P.dirty = false;
    emit();
    toast(`Progetto salvato e applicato (${r.tags} tag)`, 'ok');
    return true;
  } catch (e) {
    toast(`Progetto NON salvato:\n${e.message}`, 'error');
    return false;
  }
}

// elenchi di comodo per le select
export const areaNames = () => (P.data?.areas || []).map((a) => a.name);
export const scanNames = () => (P.data?.scan_classes || []).map((a) => a.name);
export const driverNames = () => P.drivers.map((d) => d.name);
export const udtNames = () => (P.data?.udt_types || []).map((a) => a.name);
export const TYPES = ['bool', 'u8', 'i16', 'u16', 'i32', 'u32', 'f32', 'string'];
