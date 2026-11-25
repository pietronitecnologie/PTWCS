//! Driver Siemens S7 nativo (ISO-on-TCP RFC1006 / COTP / S7comm), senza librerie C.
//! Compatibile con S7-300/400/1200/1500 (su 1200/1500: abilitare PUT/GET e
//! disattivare "accesso ottimizzato al blocco" sui DB usati).
//!
//! Indirizzi:
//! - `DB10.DBX0.1` (bool), `DB10.DBB4` (u8 / inizio string), `DB10.DBW2`, `DB10.DBD6`
//! - `M10.0`, `MB10`, `MW20`, `MD30`; `I0.0`/`E0.0`, `IW64`; `Q0.0`/`A0.0`, `QW64`
//!
//! Le stringhe sono STRING S7 (2 byte di header max/len + `len` caratteri).

use super::{Driver, group_ranges};
use crate::config::{DriverConfig, TagDef};
use crate::value::{DataType, Value};
use anyhow::{Context, Result, anyhow, bail, ensure};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const AREA_I: u8 = 0x81;
const AREA_Q: u8 = 0x82;
const AREA_M: u8 = 0x83;
const AREA_DB: u8 = 0x84;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct S7Addr {
    pub area: u8,
    pub db: u16,
    pub byte: u32,
    pub bit: u8,
}

fn split_num(s: &str) -> Result<(u32, Option<u8>)> {
    let (b, bit) = match s.split_once('.') {
        Some((b, x)) => (b, Some(x.parse::<u8>()?)),
        None => (s, None),
    };
    if let Some(x) = bit {
        ensure!(x <= 7, "bit > 7");
    }
    Ok((b.parse::<u32>()?, bit))
}

pub fn parse_addr(s: &str) -> Result<S7Addr> {
    let u = s.trim().to_ascii_uppercase();
    let strip_size = |r: &str| -> String {
        let r = r.trim_start_matches(['X', 'B', 'W', 'D']);
        r.to_string()
    };
    let res = (|| -> Result<S7Addr> {
        if let Some(rest) = u.strip_prefix("DB") {
            let (dbn, off) = rest.split_once('.').ok_or_else(|| anyhow!("manca offset"))?;
            let db = dbn.parse::<u16>()?;
            let off = off.strip_prefix("DB").unwrap_or(off);
            let (byte, bit) = split_num(&strip_size(off))?;
            return Ok(S7Addr { area: AREA_DB, db, byte, bit: bit.unwrap_or(0) });
        }
        let (area, rest) = match u.chars().next() {
            Some('M') => (AREA_M, &u[1..]),
            Some('I') | Some('E') => (AREA_I, &u[1..]),
            Some('Q') | Some('A') => (AREA_Q, &u[1..]),
            _ => bail!("area sconosciuta"),
        };
        let (byte, bit) = split_num(&strip_size(rest))?;
        Ok(S7Addr { area, db: 0, byte, bit: bit.unwrap_or(0) })
    })();
    res.with_context(|| format!("indirizzo S7 non valido: {s}"))
}

/// Byte occupati da un tag in memoria PLC.
fn s7_size(t: &TagDef) -> u32 {
    match t.dtype {
        DataType::String => t.len as u32 + 2,
        dt => dt.byte_size(0) as u32,
    }
}

fn decode(t: &TagDef, buf: &[u8], bit: u8) -> Result<Value> {
    if t.dtype == DataType::String {
        ensure!(buf.len() >= 2, "string troppo corta");
        let n = (buf[1] as usize).min(buf[0] as usize).min(buf.len() - 2);
        return Ok(Value::Str(String::from_utf8_lossy(&buf[2..2 + n]).into_owned()));
    }
    Value::decode_be(t.dtype, buf, bit)
}

fn encode(t: &TagDef, v: &Value) -> Result<Vec<u8>> {
    if t.dtype == DataType::String {
        let s = v.to_string();
        let mut b = s.as_bytes().to_vec();
        b.truncate(t.len.min(254));
        let mut out = vec![t.len.min(254) as u8, b.len() as u8];
        out.extend(b);
        out.resize(t.len.min(254) + 2, 0);
        return Ok(out);
    }
    v.encode_be(t.dtype, t.len)
}

pub struct S7Driver {
    host: String,
    port: u16,
    rack: u16,
    slot: u16,
    addrs: HashMap<String, S7Addr>,
    stream: Option<TcpStream>,
    pdu_size: usize,
    pdu_ref: u16,
}

impl S7Driver {
    pub fn new(cfg: &DriverConfig, tags: &[TagDef]) -> Result<Self> {
        let mut addrs = HashMap::new();
        for t in tags {
            let a = parse_addr(&t.address).with_context(|| format!("tag {}", t.name))?;
            addrs.insert(t.name.clone(), a);
        }
        Ok(Self {
            host: cfg.host.clone(),
            port: cfg.port.unwrap_or(102),
            rack: cfg.rack,
            slot: cfg.slot,
            addrs,
            stream: None,
            pdu_size: 240,
            pdu_ref: 0,
        })
    }

    async fn send_tpkt(&mut self, payload: &[u8]) -> Result<()> {
        let s = self.stream.as_mut().ok_or_else(|| anyhow!("non connesso"))?;
        let len = (payload.len() + 4) as u16;
        let mut pkt = vec![3, 0, (len >> 8) as u8, len as u8];
        pkt.extend_from_slice(payload);
        s.write_all(&pkt).await?;
        Ok(())
    }

    async fn recv_tpkt(&mut self) -> Result<Vec<u8>> {
        let s = self.stream.as_mut().ok_or_else(|| anyhow!("non connesso"))?;
        let mut h = [0u8; 4];
        s.read_exact(&mut h).await?;
        ensure!(h[0] == 3, "TPKT non valido");
        let len = u16::from_be_bytes([h[2], h[3]]) as usize;
        ensure!((7..=4096).contains(&len), "lunghezza TPKT non valida: {len}");
        let mut body = vec![0u8; len - 4];
        s.read_exact(&mut body).await?;
        Ok(body)
    }

    /// Invia un job S7 e ritorna (parametri, dati) della risposta.
    async fn s7_request(&mut self, param: &[u8], data: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        self.pdu_ref = self.pdu_ref.wrapping_add(1);
        let r = self.pdu_ref;
        let mut p = vec![0x02, 0xF0, 0x80]; // COTP DT
        p.extend_from_slice(&[0x32, 0x01, 0, 0, (r >> 8) as u8, r as u8]);
        p.extend_from_slice(&(param.len() as u16).to_be_bytes());
        p.extend_from_slice(&(data.len() as u16).to_be_bytes());
        p.extend_from_slice(param);
        p.extend_from_slice(data);
        self.send_tpkt(&p).await?;
        let body = self.recv_tpkt().await?;
        let cotp_len = body[0] as usize + 1;
        let s7 = body.get(cotp_len..).ok_or_else(|| anyhow!("risposta troppo corta"))?;
        ensure!(s7.len() >= 12 && s7[0] == 0x32, "header S7 non valido");
        ensure!(s7[1] == 0x03 || s7[1] == 0x02, "tipo PDU inatteso {:#x}", s7[1]);
        let (ec, code) = (s7[10], s7[11]);
        ensure!(ec == 0 && code == 0, "errore S7 classe {ec:#04x} codice {code:#04x}");
        let resp_ref = u16::from_be_bytes([s7[4], s7[5]]);
        ensure!(resp_ref == r, "pdu ref non corrispondente");
        let pl = u16::from_be_bytes([s7[6], s7[7]]) as usize;
        let dl = u16::from_be_bytes([s7[8], s7[9]]) as usize;
        ensure!(s7.len() >= 12 + pl + dl, "risposta S7 troncata");
        Ok((s7[12..12 + pl].to_vec(), s7[12 + pl..12 + pl + dl].to_vec()))
    }

    /// Legge più aree in un'unica richiesta. items: (area, db, start, len)
    async fn read_multi(&mut self, items: &[(u8, u16, u32, u32)]) -> Result<Vec<Vec<u8>>> {
        let mut param = vec![0x04, items.len() as u8];
        for &(area, db, start, len) in items {
            let a = start * 8;
            param.extend_from_slice(&[0x12, 0x0A, 0x10, 0x02]);
            param.extend_from_slice(&(len as u16).to_be_bytes());
            param.extend_from_slice(&db.to_be_bytes());
            param.extend_from_slice(&[area, (a >> 16) as u8, (a >> 8) as u8, a as u8]);
        }
        let (_, data) = self.s7_request(&param, &[]).await?;
        let mut out = Vec::with_capacity(items.len());
        let mut i = 0;
        for (k, it) in items.iter().enumerate() {
            ensure!(data.len() >= i + 4, "dati lettura troncati");
            let (rc, ts) = (data[i], data[i + 1]);
            let raw = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            ensure!(rc == 0xFF, "errore lettura area {:#x} DB{} byte {}: codice {rc:#04x}", it.0, it.1, it.2);
            let n = match ts {
                0x03 => raw.div_ceil(8),
                0x04 | 0x05 => raw / 8,
                _ => raw,
            };
            ensure!(data.len() >= i + 4 + n, "dati lettura troncati");
            out.push(data[i + 4..i + 4 + n].to_vec());
            i += 4 + n;
            if n % 2 == 1 && k + 1 < items.len() {
                i += 1;
            }
        }
        Ok(out)
    }

    async fn write_item(&mut self, a: S7Addr, bit: bool, bytes: &[u8]) -> Result<()> {
        let (ts, count, addr, dts, dlen) = if bit {
            (0x01u8, 1u16, a.byte * 8 + a.bit as u32, 0x03u8, 1u16)
        } else {
            (0x02, bytes.len() as u16, a.byte * 8, 0x04, (bytes.len() * 8) as u16)
        };
        let mut param = vec![0x05, 0x01, 0x12, 0x0A, 0x10, ts];
        param.extend_from_slice(&count.to_be_bytes());
        param.extend_from_slice(&a.db.to_be_bytes());
        param.extend_from_slice(&[a.area, (addr >> 16) as u8, (addr >> 8) as u8, addr as u8]);
        let mut data = vec![0x00, dts];
        data.extend_from_slice(&dlen.to_be_bytes());
        data.extend_from_slice(bytes);
        let (_, resp) = self.s7_request(&param, &data).await?;
        ensure!(resp.first() == Some(&0xFF), "scrittura rifiutata: codice {:#04x}", resp.first().copied().unwrap_or(0));
        Ok(())
    }
}

#[async_trait]
impl Driver for S7Driver {
    async fn connect(&mut self) -> Result<()> {
        let s = TcpStream::connect((self.host.as_str(), self.port)).await?;
        s.set_nodelay(true)?;
        self.stream = Some(s);
        // COTP Connection Request (TSAP locale 0x0100, remoto 0x01 rack/slot = PG)
        let rt = 0x0100u16 | (self.rack * 0x20 + self.slot);
        let cr = [
            0x11, 0xE0, 0x00, 0x00, 0x00, 0x01, 0x00, 0xC0, 0x01, 0x0A, 0xC1, 0x02, 0x01, 0x00, 0xC2, 0x02,
            (rt >> 8) as u8, rt as u8,
        ];
        self.send_tpkt(&cr).await?;
        let cc = self.recv_tpkt().await?;
        ensure!(cc.len() >= 2 && cc[1] == 0xD0, "connessione COTP rifiutata (rack/slot errati?)");
        // S7 Setup Communication
        let (param, _) = self.s7_request(&[0xF0, 0x00, 0x00, 0x01, 0x00, 0x01, 0x03, 0xC0], &[]).await?;
        ensure!(param.len() >= 8 && param[0] == 0xF0, "setup communication fallito");
        self.pdu_size = u16::from_be_bytes([param[6], param[7]]) as usize;
        ensure!(self.pdu_size >= 64, "PDU troppo piccola: {}", self.pdu_size);
        tracing::debug!("S7 PDU negoziata: {}", self.pdu_size);
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(mut s) = self.stream.take() {
            let _ = s.shutdown().await;
        }
    }

    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>> {
        // Raggruppa per (area, db) e poi per blocchi contigui.
        let max_block = (self.pdu_size - 30) as u32;
        let mut groups: HashMap<(u8, u16), Vec<usize>> = HashMap::new();
        for (i, t) in tags.iter().enumerate() {
            let a = self.addrs[&t.name];
            groups.entry((a.area, a.db)).or_default().push(i);
        }
        // blocks: (area, db, start, len), e mappa tag -> indice blocco
        let mut blocks: Vec<(u8, u16, u32, u32)> = Vec::new();
        let mut tag_block = vec![0usize; tags.len()];
        for ((area, db), idx) in &groups {
            let ranges: Vec<_> = idx.iter().map(|&i| (self.addrs[&tags[i].name].byte, s7_size(&tags[i]))).collect();
            let (bl, assign) = group_ranges(&ranges, 16, max_block);
            let base = blocks.len();
            for (s, l) in bl {
                ensure!(l <= max_block, "tag oltre la dimensione PDU");
                blocks.push((*area, *db, s, l));
            }
            for (k, &i) in idx.iter().enumerate() {
                tag_block[i] = base + assign[k];
            }
        }
        // Impacchetta più blocchi per richiesta rispettando la PDU.
        let mut data: Vec<Vec<u8>> = Vec::with_capacity(blocks.len());
        let mut start = 0;
        while start < blocks.len() {
            let mut end = start;
            let mut resp = 14usize;
            while end < blocks.len() && end - start < 19 {
                let need = 4 + blocks[end].3 as usize + 1;
                if resp + need > self.pdu_size || 12 + 12 * (end - start + 1) > self.pdu_size {
                    break;
                }
                resp += need;
                end += 1;
            }
            let chunk = blocks[start..end].to_vec();
            data.extend(self.read_multi(&chunk).await?);
            start = end;
        }
        tags.iter()
            .enumerate()
            .map(|(i, t)| {
                let a = self.addrs[&t.name];
                let b = blocks[tag_block[i]];
                let off = (a.byte - b.2) as usize;
                let buf = data[tag_block[i]].get(off..off + s7_size(t) as usize).ok_or_else(|| anyhow!("dati mancanti per {}", t.name))?;
                decode(t, buf, a.bit)
            })
            .collect()
    }

    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()> {
        let a = self.addrs[&tag.name];
        if tag.dtype == DataType::Bool {
            let v = value.as_bool().ok_or_else(|| anyhow!("bool atteso"))?;
            return self.write_item(a, true, &[v as u8]).await;
        }
        let bytes = encode(tag, value)?;
        let chunk = self.pdu_size - 36;
        for (k, c) in bytes.chunks(chunk).enumerate() {
            let sub = S7Addr { byte: a.byte + (k * chunk) as u32, ..a };
            self.write_item(sub, false, c).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses() {
        assert_eq!(parse_addr("DB10.DBX4.3").unwrap(), S7Addr { area: AREA_DB, db: 10, byte: 4, bit: 3 });
        assert_eq!(parse_addr("db1.dbw20").unwrap(), S7Addr { area: AREA_DB, db: 1, byte: 20, bit: 0 });
        assert_eq!(parse_addr("MW10").unwrap(), S7Addr { area: AREA_M, db: 0, byte: 10, bit: 0 });
        assert_eq!(parse_addr("E1.7").unwrap(), S7Addr { area: AREA_I, db: 0, byte: 1, bit: 7 });
        assert!(parse_addr("Z1").is_err());
        assert!(parse_addr("M1.9").is_err());
    }
}
