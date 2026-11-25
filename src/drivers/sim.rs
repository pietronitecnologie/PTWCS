//! Driver simulato: memoria interna, utile per sviluppare HMI e logiche
//! senza PLC. I valori scritti vengono riletti al ciclo successivo.

use super::Driver;
use crate::config::TagDef;
use crate::value::{DataType, Value};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct SimDriver {
    mem: HashMap<String, Value>,
}

impl SimDriver {
    pub fn new(tags: &[TagDef]) -> Self {
        let mem = tags
            .iter()
            .map(|t| {
                let v = t.initial.clone().and_then(|v| v.coerce(t.dtype).ok()).unwrap_or(match t.dtype {
                    DataType::Bool => Value::Bool(false),
                    DataType::F32 => Value::Float(0.0),
                    DataType::String => Value::Str(String::new()),
                    _ => Value::Int(0),
                });
                (t.address.clone(), v)
            })
            .collect();
        Self { mem }
    }
}

#[async_trait]
impl Driver for SimDriver {
    async fn connect(&mut self) -> Result<()> {
        Ok(())
    }

    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>> {
        Ok(tags.iter().map(|t| self.mem.get(&t.address).cloned().unwrap_or_default()).collect())
    }

    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()> {
        self.mem.insert(tag.address.clone(), value.clone());
        Ok(())
    }
}
