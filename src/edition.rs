//! Edizioni di distribuzione del runtime: quali moduli sono inclusi.
//!
//! Il nucleo **Tag** (driver, tag, allarmi, storico, trend, script, sicurezza, audit) è
//! sempre presente. Si aggiungono:
//! - **scada**: viste HMI, simboli e faceplate, processo (fasi, ricette, batch);
//! - **wcs**: punti di decisione, regole di smistamento, classi e WMS, magazzini automatici;
//! - **sim**: simulazione fisica sul layout delle viste (richiede scada).
//!
//! L'edizione si sceglie in `plant.yaml` (`edition: scada-sim`) e può essere fissata alla
//! compilazione con la variabile `PTWCS_EDITION`: in quel caso la configurazione può solo
//! togliere moduli, non aggiungerne (pacchetti distribuiti con `tools/dist.sh`).

use anyhow::{Result, bail};
use serde::Serialize;

/// Edizione fissata alla compilazione (vuota = nessun limite).
pub const BUILD_EDITION: Option<&str> = option_env!("PTWCS_EDITION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Modules {
    pub scada: bool,
    pub wcs: bool,
    pub sim: bool,
}

impl Modules {
    pub const ALL: Modules = Modules { scada: true, wcs: true, sim: true };

    /// `full`, `tag`, `scada`, `wcs`, `scada-sim`, `scada+wcs`, `tag+scada+sim`…
    pub fn parse(s: &str) -> Result<Modules> {
        let mut m = Modules { scada: false, wcs: false, sim: false };
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() {
            bail!("edizione vuota");
        }
        for tok in s.split(['+', '-', ',', ' ']).filter(|t| !t.is_empty()) {
            match tok {
                "full" | "completa" | "all" => m = Modules::ALL,
                "tag" | "tags" => {}
                "scada" | "hmi" => m.scada = true,
                "wcs" | "vcs" => m.wcs = true,
                "sim" | "simulazione" | "physics" | "fisica" => m.sim = true,
                x => bail!("modulo '{x}' sconosciuto nell'edizione '{s}' (validi: tag, scada, wcs, sim, full)"),
            }
        }
        if m.sim {
            // la simulazione fisica vive sul layout delle viste SCADA
            m.scada = true;
        }
        Ok(m)
    }

    fn and(self, o: Modules) -> Modules {
        Modules { scada: self.scada && o.scada, wcs: self.wcs && o.wcs, sim: self.sim && o.sim && self.scada && o.scada }
    }

    /// Nome canonico: tag, scada, wcs, scada-sim, scada-wcs, full.
    pub fn name(self) -> String {
        match (self.scada, self.wcs, self.sim) {
            (true, true, true) => "full".into(),
            (false, false, false) => "tag".into(),
            _ => [("scada", self.scada), ("wcs", self.wcs), ("sim", self.sim)].iter().filter(|x| x.1).map(|x| x.0).collect::<Vec<_>>().join("-"),
        }
    }

    pub fn label(self) -> String {
        let mut v = vec!["Tag"];
        if self.scada {
            v.push("SCADA");
        }
        if self.wcs {
            v.push("WCS");
        }
        if self.sim {
            v.push("Simulazione");
        }
        v.join(" + ")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Edition {
    pub name: String,
    pub label: String,
    pub modules: Modules,
    /// Edizione fissata nel pacchetto (la configurazione non può ampliarla).
    pub locked: Option<String>,
    /// Moduli richiesti in configurazione ma non inclusi nel pacchetto.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<String>,
}

impl Edition {
    /// Edizione effettiva = quella richiesta in configurazione, limitata da quella del pacchetto.
    pub fn resolve(configured: Option<&str>, build: Option<&str>) -> Result<Edition> {
        let want = match configured {
            Some(s) => Modules::parse(s)?,
            None => match build {
                Some(b) => Modules::parse(b)?,
                None => Modules::ALL,
            },
        };
        let (m, locked) = match build.filter(|b| !b.trim().is_empty()) {
            Some(b) => (want.and(Modules::parse(b)?), Some(Modules::parse(b)?.name())),
            None => (want, None),
        };
        let mut excluded = vec![];
        for (n, w, h) in [("scada", want.scada, m.scada), ("wcs", want.wcs, m.wcs), ("sim", want.sim, m.sim)] {
            if w && !h {
                excluded.push(n.to_string());
            }
        }
        Ok(Edition { name: m.name(), label: m.label(), modules: m, locked, excluded })
    }

    pub fn full() -> Edition {
        Edition::resolve(None, None).unwrap()
    }

    /// Modulo necessario per un'API (None = nucleo Tag, sempre disponibile).
    pub fn module_of(path: &str) -> Option<&'static str> {
        let area = path.strip_prefix("/api/")?.split('/').next().unwrap_or("");
        Some(match area {
            "views" | "symbols" | "proc" | "phase" | "inst" | "recipes" | "batches" => "scada",
            "wh" | "sorting" | "rules" | "decisions" => "wcs",
            "physics" => "sim",
            _ => return None,
        })
    }

    pub fn has(&self, module: &str) -> bool {
        match module {
            "scada" => self.modules.scada,
            "wcs" => self.modules.wcs,
            "sim" => self.modules.sim,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_lock() {
        assert_eq!(Modules::parse("full").unwrap(), Modules::ALL);
        assert_eq!(Modules::parse("tag+scada").unwrap().name(), "scada");
        assert_eq!(Modules::parse("Tag + VCS").unwrap().name(), "wcs");
        assert_eq!(Modules::parse("sim").unwrap().name(), "scada-sim", "sim richiede scada");
        assert_eq!(Modules::parse("scada-sim").unwrap().label(), "Tag + SCADA + Simulazione");
        assert_eq!(Modules::parse("tag").unwrap().name(), "tag");
        assert!(Modules::parse("erp").is_err());
        // senza limiti: quello che chiede la configurazione (default completa)
        assert_eq!(Edition::resolve(None, None).unwrap().name, "full");
        assert_eq!(Edition::resolve(Some("wcs"), None).unwrap().name, "wcs");
        // pacchetto SCADA: la configurazione non può aggiungere il WCS
        let e = Edition::resolve(Some("full"), Some("scada")).unwrap();
        assert_eq!(e.name, "scada");
        assert_eq!(e.locked.as_deref(), Some("scada"));
        assert_eq!(e.excluded, vec!["wcs", "sim"]);
        assert_eq!(Edition::resolve(None, Some("scada-sim")).unwrap().name, "scada-sim");
        assert_eq!(Edition::resolve(Some("scada"), Some("full")).unwrap().name, "scada", "si può sempre restringere");
        assert_eq!(Edition::module_of("/api/wh/MP1/summary"), Some("wcs"));
        assert_eq!(Edition::module_of("/api/physics/start"), Some("sim"));
        assert_eq!(Edition::module_of("/api/views"), Some("scada"));
        assert_eq!(Edition::module_of("/api/tags"), None);
    }
}
