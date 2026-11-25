//! Tag database: immagine live di tutti i tag, con qualità e timestamp.
//! È il punto di incontro tra driver (che scrivono letture), HMI, allarmi e
//! decision engine (che leggono e inviano comandi di scrittura).

use crate::config::TagDef;
use crate::value::Value;
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// Mai letto dall'avvio.
    Init,
    Good,
    /// Comunicazione persa o errore di lettura.
    Bad,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagState {
    pub value: Value,
    pub quality: Quality,
    /// Timestamp ultimo cambio (ms epoch).
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagUpdate {
    pub name: String,
    #[serde(flatten)]
    pub state: TagState,
}

/// Comando di scrittura inviato al task del driver.
pub struct WriteCmd {
    pub tag: TagDef,
    pub value: Value,
    pub ack: oneshot::Sender<Result<()>>,
}

/// Definizioni dei tag (ordine di configurazione + indice per nome).
#[derive(Default)]
struct Defs {
    list: Vec<TagDef>,
    idx: HashMap<String, usize>,
}

impl Defs {
    fn new(list: Vec<TagDef>) -> Self {
        let idx = list.iter().enumerate().map(|(i, d)| (d.name.clone(), i)).collect();
        Self { list, idx }
    }
}

pub struct TagDb {
    defs: RwLock<Defs>,
    states: RwLock<HashMap<String, TagState>>,
    tx: broadcast::Sender<TagUpdate>,
    writers: RwLock<HashMap<String, mpsc::Sender<WriteCmd>>>,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl TagDb {
    pub fn new(defs: &[TagDef]) -> Arc<Self> {
        let (tx, _) = broadcast::channel(8192);
        let db = Arc::new(Self {
            defs: RwLock::new(Defs::default()),
            states: RwLock::new(HashMap::new()),
            tx,
            writers: RwLock::new(HashMap::new()),
        });
        db.set_defs(defs.to_vec());
        db
    }

    /// Sostituisce le definizioni (modifica a caldo dal Designer). I tag
    /// nuovi partono in qualità Init, quelli rimossi spariscono, quelli
    /// rimasti conservano il valore.
    pub fn set_defs(&self, list: Vec<TagDef>) {
        {
            let mut st = self.states.write().unwrap();
            st.retain(|k, _| list.iter().any(|d| &d.name == k));
            for d in &list {
                st.entry(d.name.clone())
                    .or_insert_with(|| TagState { value: Value::Null, quality: Quality::Init, ts: now_ms() });
            }
        }
        *self.defs.write().unwrap() = Defs::new(list);
    }

    /// Registra (o sostituisce) la coda di scrittura di un driver.
    pub fn set_writer(&self, driver: &str, tx: Option<mpsc::Sender<WriteCmd>>) {
        let mut w = self.writers.write().unwrap();
        match tx {
            Some(tx) => {
                w.insert(driver.to_string(), tx);
            }
            None => {
                w.remove(driver);
            }
        }
    }

    /// Elenco delle definizioni nell'ordine di configurazione.
    pub fn defs(&self) -> Vec<TagDef> {
        self.defs.read().unwrap().list.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TagUpdate> {
        self.tx.subscribe()
    }

    pub fn def(&self, name: &str) -> Option<TagDef> {
        let d = self.defs.read().unwrap();
        d.idx.get(name).map(|&i| d.list[i].clone())
    }

    pub fn get(&self, name: &str) -> Option<TagState> {
        self.states.read().unwrap().get(name).cloned()
    }

    pub fn value(&self, name: &str) -> Value {
        self.get(name).map(|s| s.value).unwrap_or_default()
    }

    pub fn snapshot(&self) -> HashMap<String, TagState> {
        self.states.read().unwrap().clone()
    }

    /// Aggiorna un tag (chiamato dai driver). Notifica solo se cambia.
    pub fn update(&self, name: &str, value: Value, quality: Quality) {
        let changed = {
            let mut st = self.states.write().unwrap();
            let Some(s) = st.get_mut(name) else { return };
            if s.value == value && s.quality == quality {
                None
            } else {
                s.value = value;
                s.quality = quality;
                s.ts = now_ms();
                Some(s.clone())
            }
        };
        if let Some(state) = changed {
            let _ = self.tx.send(TagUpdate { name: name.to_string(), state });
        }
    }

    /// Marca come Bad tutti i tag di un driver (connessione persa).
    pub fn set_driver_bad(&self, driver: &str) {
        let names: Vec<_> = self
            .defs
            .read()
            .unwrap()
            .list
            .iter()
            .filter(|d| d.driver == driver)
            .map(|d| d.name.clone())
            .collect();
        for n in names {
            let v = self.value(&n);
            self.update(&n, v, Quality::Bad);
        }
    }

    /// Scrive un valore sul PLC tramite il driver proprietario del tag.
    /// Ritorna solo dopo la conferma del driver (o errore/timeout).
    pub async fn write(&self, name: &str, value: Value) -> Result<()> {
        let def = self.def(name).ok_or_else(|| anyhow!("tag sconosciuto: {name}"))?;
        if !def.writable {
            return Err(anyhow!("tag {name} non scrivibile"));
        }
        let value = value.coerce(def.dtype)?;
        let tx = self
            .writers
            .read()
            .unwrap()
            .get(&def.driver)
            .cloned()
            .ok_or_else(|| anyhow!("driver {} non attivo", def.driver))?;
        let (ack, rx) = oneshot::channel();
        tx.send(WriteCmd { tag: def.clone(), value: value.clone(), ack })
            .await
            .map_err(|_| anyhow!("driver {} terminato", def.driver))?;
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .context("timeout scrittura")?
            .map_err(|_| anyhow!("driver {} non ha risposto", def.driver))??;
        // Aggiornamento ottimistico: il prossimo poll confermerà il valore.
        self.update(name, value, Quality::Good);
        Ok(())
    }
}
