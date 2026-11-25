//! Driver OPC UA (client, async-opcua). Compilare con `--features opcua`.
//! Indirizzo del tag = NodeId, es. `ns=3;s="DB_WCS"."ReqID"` oppure `ns=2;i=1001`.
//! Sicurezza: per ora endpoint senza cifratura (SecurityPolicy None, utente anonimo).

use super::Driver;
use crate::config::{DriverConfig, TagDef};
use crate::value::{DataType, Value};
use anyhow::{Context, Result, anyhow, bail};
use opcua::client::{ClientBuilder, IdentityToken, Session};
use opcua::types::{
    AttributeId, DataValue, EndpointDescription, MessageSecurityMode, NodeId, ReadValueId, TimestampsToReturn,
    UAString, UserTokenPolicy, Variant, WriteValue,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub struct OpcUaDriver {
    endpoint: String,
    timeout: Duration,
    nodes: HashMap<String, NodeId>,
    session: Option<Arc<Session>>,
    event_loop: Option<tokio::task::JoinHandle<opcua::types::StatusCode>>,
}

impl OpcUaDriver {
    pub fn new(cfg: &DriverConfig, tags: &[TagDef]) -> Result<Self> {
        if cfg.endpoint.is_empty() {
            bail!("driver {}: manca 'endpoint' (es. opc.tcp://host:4840)", cfg.name);
        }
        let mut nodes = HashMap::new();
        for t in tags {
            let n = NodeId::from_str(&t.address)
                .map_err(|e| anyhow!("tag {}: NodeId non valido '{}': {e:?}", t.name, t.address))?;
            nodes.insert(t.name.clone(), n);
        }
        Ok(Self {
            endpoint: cfg.endpoint.clone(),
            timeout: Duration::from_millis(cfg.timeout_ms),
            nodes,
            session: None,
            event_loop: None,
        })
    }

    fn session(&self) -> Result<&Arc<Session>> {
        self.session.as_ref().ok_or_else(|| anyhow!("non connesso"))
    }
}

fn from_variant(v: &Variant) -> Value {
    match v {
        Variant::Boolean(b) => Value::Bool(*b),
        Variant::SByte(x) => Value::Int(*x as i64),
        Variant::Byte(x) => Value::Int(*x as i64),
        Variant::Int16(x) => Value::Int(*x as i64),
        Variant::UInt16(x) => Value::Int(*x as i64),
        Variant::Int32(x) => Value::Int(*x as i64),
        Variant::UInt32(x) => Value::Int(*x as i64),
        Variant::Int64(x) => Value::Int(*x),
        Variant::UInt64(x) => Value::Int(*x as i64),
        Variant::Float(x) => Value::Float(*x as f64),
        Variant::Double(x) => Value::Float(*x),
        Variant::String(s) => Value::Str(s.as_ref().to_string()),
        Variant::Empty => Value::Null,
        other => Value::Str(format!("{other:?}")),
    }
}

fn to_variant(t: &TagDef, v: &Value) -> Result<Variant> {
    let v = v.coerce(t.dtype)?;
    Ok(match (t.dtype, v) {
        (DataType::Bool, Value::Bool(b)) => Variant::Boolean(b),
        (DataType::U8, Value::Int(i)) => Variant::Byte(i as u8),
        (DataType::I16, Value::Int(i)) => Variant::Int16(i as i16),
        (DataType::U16, Value::Int(i)) => Variant::UInt16(i as u16),
        (DataType::I32, Value::Int(i)) => Variant::Int32(i as i32),
        (DataType::U32, Value::Int(i)) => Variant::UInt32(i as u32),
        (DataType::F32, Value::Float(x)) => Variant::Float(x as f32),
        (DataType::String, Value::Str(s)) => Variant::String(UAString::from(s)),
        (dt, v) => bail!("conversione non supportata {dt:?} {v:?}"),
    })
}

#[async_trait]
impl Driver for OpcUaDriver {
    async fn connect(&mut self) -> Result<()> {
        self.disconnect().await;
        let mut client = ClientBuilder::new()
            .application_name("WCS-SCADA")
            .application_uri("urn:wcs-scada")
            .product_uri("urn:wcs-scada")
            .trust_server_certs(true)
            .session_retry_limit(0)
            .request_timeout(self.timeout)
            .client()
            .map_err(|e| anyhow!("configurazione client OPC UA: {e:?}"))?;
        let ep: EndpointDescription =
            (self.endpoint.as_str(), "None", MessageSecurityMode::None, UserTokenPolicy::anonymous()).into();
        let (session, event_loop) = client
            .connect_to_matching_endpoint(ep, IdentityToken::Anonymous)
            .await
            .map_err(|e| anyhow!("connessione OPC UA: {e}"))?;
        self.event_loop = Some(event_loop.spawn());
        if !session.wait_for_connection().await {
            bail!("sessione OPC UA non stabilita");
        }
        self.session = Some(session);
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = tokio::time::timeout(Duration::from_secs(1), s.disconnect()).await;
        }
        if let Some(h) = self.event_loop.take() {
            h.abort();
        }
    }

    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>> {
        let ids: Vec<ReadValueId> = tags.iter().map(|t| ReadValueId::from(&self.nodes[&t.name])).collect();
        let mut out = Vec::with_capacity(tags.len());
        // blocchi da 500 nodi per richiesta
        for (chunk_ids, chunk_tags) in ids.chunks(500).zip(tags.chunks(500)) {
            let res = self.session()?.read(chunk_ids, TimestampsToReturn::Neither, 0.0).await.context("read OPC UA")?;
            if res.len() != chunk_tags.len() {
                bail!("risposta OPC UA incompleta");
            }
            for (dv, t) in res.iter().zip(chunk_tags) {
                if let Some(st) = dv.status
                    && !st.is_good() {
                        bail!("tag {}: stato {st}", t.name);
                    }
                let v = dv.value.as_ref().map(from_variant).unwrap_or(Value::Null);
                out.push(if t.dtype == DataType::String { Value::Str(v.to_string()) } else { v });
            }
        }
        Ok(out)
    }

    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()> {
        let wv = WriteValue {
            node_id: self.nodes[&tag.name].clone(),
            attribute_id: AttributeId::Value as u32,
            index_range: Default::default(),
            value: DataValue::value_only(to_variant(tag, value)?),
        };
        let st = self.session()?.write(&[wv]).await.context("write OPC UA")?;
        match st.first() {
            Some(s) if s.is_good() => Ok(()),
            Some(s) => bail!("scrittura rifiutata: {s}"),
            None => bail!("nessuna risposta alla scrittura"),
        }
    }
}
