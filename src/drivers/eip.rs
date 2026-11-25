//! Driver Allen-Bradley EtherNet/IP (CIP, controller Logix) basato su rseip.
//! Compilare con `--features eip`. Indirizzo = nome tag Logix,
//! es. `WCS_ReqID`, `Program:Main.Scanner.ReqID`, `Counters[3]`.
//! Tipi atomici: BOOL, SINT, INT, DINT, REAL. Le STRING Logix sono in sola lettura.

use super::Driver;
use crate::config::{DriverConfig, TagDef};
use crate::value::{DataType, Value};
use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use bytes::Bytes;
use rseip::client::ab_eip::*;
use rseip::precludes::*;

pub struct EipDriver {
    host: String,
    slot: u8,
    client: Option<AbEipClient>,
}

impl EipDriver {
    pub fn new(cfg: &DriverConfig, tags: &[TagDef]) -> Result<Self> {
        for t in tags {
            EPath::parse_tag(&t.address).map_err(|e| anyhow!("tag {}: indirizzo non valido: {e}", t.name))?;
            if t.writable && t.dtype == DataType::String {
                bail!("tag {}: la scrittura di STRING Logix non è supportata", t.name);
            }
        }
        if cfg.host.is_empty() {
            bail!("driver {}: manca 'host'", cfg.name);
        }
        Ok(Self { host: cfg.host.clone(), slot: cfg.slot as u8, client: None })
    }

    fn client(&mut self) -> Result<&mut AbEipClient> {
        self.client.as_mut().ok_or_else(|| anyhow!("non connesso"))
    }
}

fn decode(t: &TagDef, tv: &TagValue<Bytes>) -> Result<Value> {
    let b = &tv.value[..];
    let need = |n: usize| if b.len() < n { Err(anyhow!("dati troppo corti per {}", t.name)) } else { Ok(()) };
    Ok(match tv.tag_type {
        TagType::Bool => {
            need(1)?;
            Value::Bool(b[0] != 0)
        }
        TagType::Sint => {
            need(1)?;
            Value::Int(b[0] as i8 as i64)
        }
        TagType::Int => {
            need(2)?;
            Value::Int(i16::from_le_bytes([b[0], b[1]]) as i64)
        }
        TagType::Dint | TagType::Dword => {
            need(4)?;
            let v = i32::from_le_bytes(b[..4].try_into()?);
            Value::Int(if t.dtype == DataType::U32 { v as u32 as i64 } else { v as i64 })
        }
        TagType::Lint => {
            need(8)?;
            Value::Int(i64::from_le_bytes(b[..8].try_into()?))
        }
        TagType::Real => {
            need(4)?;
            Value::Float(f32::from_le_bytes(b[..4].try_into()?) as f64)
        }
        // STRING Logix: DINT LEN + SINT[82]
        TagType::Structure(_) if t.dtype == DataType::String => {
            need(4)?;
            let n = (i32::from_le_bytes(b[..4].try_into()?).max(0) as usize).min(b.len() - 4);
            Value::Str(String::from_utf8_lossy(&b[4..4 + n]).into_owned())
        }
        other => bail!("tipo CIP non supportato per {}: {other:?}", t.name),
    })
}

#[async_trait]
impl Driver for EipDriver {
    async fn connect(&mut self) -> Result<()> {
        let path = PortSegment { port: 1, link: Bytes::from(vec![self.slot]) };
        let c = AbEipClient::new_host_lookup(&self.host).await?.with_connection_path(path);
        self.client = Some(c);
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(mut c) = self.client.take() {
            let _ = c.close().await;
        }
    }

    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>> {
        // Nota: una richiesta per tag. Per molti tag conviene raggrupparli in una
        // struttura (UDT) Logix e leggerla come blocco.
        let mut out = Vec::with_capacity(tags.len());
        for t in tags {
            let path = EPath::parse_tag(&t.address)?;
            let tv: TagValue<Bytes> = self.client()?.read_tag(path).await.map_err(|e| anyhow!("{}: {e}", t.name))?;
            out.push(decode(t, &tv)?);
        }
        Ok(out)
    }

    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()> {
        let path = EPath::parse_tag(&tag.address)?;
        let v = value.coerce(tag.dtype)?;
        let c = self.client()?;
        let r = match (tag.dtype, v) {
            (DataType::Bool, Value::Bool(b)) => c.write_tag(path, TagValue { tag_type: TagType::Bool, value: b }).await,
            (DataType::U8, Value::Int(i)) => c.write_tag(path, TagValue { tag_type: TagType::Sint, value: i as i8 }).await,
            (DataType::I16 | DataType::U16, Value::Int(i)) => {
                c.write_tag(path, TagValue { tag_type: TagType::Int, value: i as i16 }).await
            }
            (DataType::I32 | DataType::U32, Value::Int(i)) => {
                c.write_tag(path, TagValue { tag_type: TagType::Dint, value: i as i32 }).await
            }
            (DataType::F32, Value::Float(x)) => c.write_tag(path, TagValue { tag_type: TagType::Real, value: x as f32 }).await,
            (dt, _) => bail!("scrittura {dt:?} non supportata"),
        };
        r.map_err(|e| anyhow!("{}: {e}", tag.name))
    }
}
