//! Driver Modbus TCP.
//!
//! Indirizzi (offset 0-based):
//! - `coil:N`   coil (R/W, bool)
//! - `di:N`     discrete input (R, bool)
//! - `hr:N`     holding register (R/W); `hr:N.b` = bit b del registro (bool)
//! - `ir:N`     input register (R);     `ir:N.b` = bit b
//!
//! I tipi a 32 bit occupano 2 registri (word alta per prima, big-endian);
//! le stringhe `len` caratteri, 2 per registro.

use super::{Driver, group_ranges};
use crate::config::{DriverConfig, TagDef};
use crate::value::{DataType, Value};
use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio_modbus::client::Context;
use tokio_modbus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Area {
    Coil,
    DiscreteInput,
    Holding,
    Input,
}

#[derive(Debug, Clone, Copy)]
struct Addr {
    area: Area,
    offset: u16,
    bit: Option<u8>,
}

fn parse_addr(s: &str) -> Result<Addr> {
    let (a, rest) = s.split_once(':').ok_or_else(|| anyhow!("indirizzo modbus non valido: {s}"))?;
    let area = match a.trim().to_ascii_lowercase().as_str() {
        "coil" | "c" => Area::Coil,
        "di" => Area::DiscreteInput,
        "hr" => Area::Holding,
        "ir" => Area::Input,
        x => bail!("area modbus sconosciuta '{x}' in {s}"),
    };
    let (off, bit) = match rest.split_once('.') {
        Some((o, b)) => (o, Some(b.trim().parse::<u8>()?)),
        None => (rest, None),
    };
    let offset = off.trim().parse::<u16>().with_context(|| format!("offset non valido in {s}"))?;
    if let Some(b) = bit
        && (b > 15 || matches!(area, Area::Coil | Area::DiscreteInput)) {
            bail!("bit non valido in {s}");
        }
    Ok(Addr { area, offset, bit })
}

/// Numero di registri occupati da un tag in area holding/input.
fn reg_count(t: &TagDef) -> u16 {
    match t.dtype {
        DataType::String => t.len.div_ceil(2) as u16,
        dt => dt.byte_size(0).div_ceil(2) as u16,
    }
}

fn words_to_bytes(w: &[u16]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_be_bytes()).collect()
}

fn bytes_to_words(b: &[u8]) -> Vec<u16> {
    b.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect()
}

pub struct ModbusDriver {
    host: String,
    port: u16,
    unit: u8,
    addrs: HashMap<String, Addr>,
    ctx: Option<Context>,
}

impl ModbusDriver {
    pub fn new(cfg: &DriverConfig, tags: &[TagDef]) -> Result<Self> {
        let mut addrs = HashMap::new();
        for t in tags {
            let a = parse_addr(&t.address).with_context(|| format!("tag {}", t.name))?;
            if matches!(a.area, Area::Coil | Area::DiscreteInput) && t.dtype != DataType::Bool {
                bail!("tag {}: coil/di supportano solo bool", t.name);
            }
            if a.bit.is_some() && t.dtype != DataType::Bool {
                bail!("tag {}: indirizzo a bit richiede tipo bool", t.name);
            }
            if t.writable && matches!(a.area, Area::DiscreteInput | Area::Input) {
                bail!("tag {}: area di sola lettura", t.name);
            }
            addrs.insert(t.name.clone(), a);
        }
        Ok(Self { host: cfg.host.clone(), port: cfg.port.unwrap_or(502), unit: cfg.unit_id, addrs, ctx: None })
    }

    fn ctx(&mut self) -> Result<&mut Context> {
        self.ctx.as_mut().ok_or_else(|| anyhow!("non connesso"))
    }

    async fn read_area(&mut self, area: Area, start: u16, cnt: u16) -> Result<Vec<u16>> {
        let ctx = self.ctx()?;
        let r = match area {
            Area::Coil => ctx.read_coils(start, cnt).await.map(|r| r.map(bools_to_u16)),
            Area::DiscreteInput => ctx.read_discrete_inputs(start, cnt).await.map(|r| r.map(bools_to_u16)),
            Area::Holding => ctx.read_holding_registers(start, cnt).await,
            Area::Input => ctx.read_input_registers(start, cnt).await,
        };
        r?.map_err(|e| anyhow!("eccezione modbus {e:?} ({area:?} {start}+{cnt})"))
    }
}

fn bools_to_u16(v: Vec<bool>) -> Vec<u16> {
    v.into_iter().map(u16::from).collect()
}

#[async_trait]
impl Driver for ModbusDriver {
    async fn connect(&mut self) -> Result<()> {
        let addr = tokio::net::lookup_host((self.host.as_str(), self.port))
            .await?
            .next()
            .ok_or_else(|| anyhow!("host non risolto: {}", self.host))?;
        self.ctx = Some(tcp::connect_slave(addr, Slave(self.unit)).await?);
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(mut c) = self.ctx.take() {
            let _ = c.disconnect().await;
        }
    }

    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>> {
        // Raggruppa per area, poi in blocchi contigui (max 120 registri / 1000 bit).
        let mut per_area: HashMap<Area, Vec<usize>> = HashMap::new();
        for (i, t) in tags.iter().enumerate() {
            per_area.entry(self.addrs[&t.name].area).or_default().push(i);
        }
        let mut out = vec![Value::Null; tags.len()];
        for (area, idx) in per_area {
            let bitwise = matches!(area, Area::Coil | Area::DiscreteInput);
            let ranges: Vec<(u32, u32)> = idx
                .iter()
                .map(|&i| {
                    let t = &tags[i];
                    let n = if bitwise { 1 } else { reg_count(t) };
                    (self.addrs[&t.name].offset as u32, n as u32)
                })
                .collect();
            let (max_gap, max_len) = if bitwise { (64, 1000) } else { (16, 120) };
            let (blocks, assign) = group_ranges(&ranges, max_gap, max_len);
            let mut data = Vec::with_capacity(blocks.len());
            for (s, l) in &blocks {
                data.push(self.read_area(area, *s as u16, *l as u16).await?);
            }
            for (k, &i) in idx.iter().enumerate() {
                let t = &tags[i];
                let a = self.addrs[&t.name];
                let (bs, _) = blocks[assign[k]];
                let words = &data[assign[k]][(a.offset as u32 - bs) as usize..];
                out[i] = if bitwise {
                    Value::Bool(words[0] != 0)
                } else if let Some(b) = a.bit {
                    Value::Bool(words[0] >> b & 1 == 1)
                } else if t.dtype == DataType::U8 {
                    Value::Int((words[0] & 0xFF) as i64)
                } else {
                    let n = reg_count(t) as usize;
                    Value::decode_be(t.dtype, &words_to_bytes(&words[..n])[..t.dtype.byte_size(t.len).max(1)], 0)?
                };
            }
        }
        Ok(out)
    }

    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()> {
        let a = self.addrs[&tag.name];
        match (a.area, a.bit) {
            (Area::Coil, _) => {
                let v = value.as_bool().ok_or_else(|| anyhow!("bool atteso"))?;
                self.ctx()?.write_single_coil(a.offset, v).await??;
            }
            (Area::Holding, Some(b)) => {
                // read-modify-write del singolo bit
                let cur = self.read_area(Area::Holding, a.offset, 1).await?[0];
                let v = value.as_bool().ok_or_else(|| anyhow!("bool atteso"))?;
                let new = if v { cur | 1 << b } else { cur & !(1 << b) };
                self.ctx()?.write_single_register(a.offset, new).await??;
            }
            (Area::Holding, None) => {
                let mut bytes = value.encode_be(tag.dtype, tag.len)?;
                if bytes.len() % 2 == 1 {
                    // u8 in un registro: valore nel byte basso
                    bytes.insert(0, 0);
                }
                let words = bytes_to_words(&bytes);
                self.ctx()?.write_multiple_registers(a.offset, &words).await??;
            }
            _ => bail!("area di sola lettura"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addr() {
        let a = parse_addr("hr:100.3").unwrap();
        assert_eq!((a.area, a.offset, a.bit), (Area::Holding, 100, Some(3)));
        assert!(parse_addr("coil:1.2").is_err());
        assert!(parse_addr("xx:1").is_err());
    }
}
