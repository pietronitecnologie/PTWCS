//! Tipi di dato dei tag e valori runtime.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Tipo dato di un tag, indipendente dal protocollo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Bool,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    /// Stringa a lunghezza fissa (numero massimo di caratteri in `TagDef::len`).
    String,
}

impl DataType {
    /// Dimensione in byte del dato nell'immagine di memoria del PLC.
    pub fn byte_size(self, str_len: usize) -> usize {
        match self {
            DataType::Bool | DataType::U8 => 1,
            DataType::I16 | DataType::U16 => 2,
            DataType::I32 | DataType::U32 | DataType::F32 => 4,
            DataType::String => str_len,
        }
    }
}

/// Valore runtime di un tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(Default)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    #[default]
    Null,
}


impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Null => write!(f, "null"),
        }
    }
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Bool(b) => Some(*b as i64),
            Value::Int(i) => Some(*i),
            Value::Float(x) => Some(*x as i64),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Null => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(*b as i64 as f64),
            Value::Int(i) => Some(*i as f64),
            Value::Float(x) => Some(*x),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Null => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            Value::Float(x) => Some(*x != 0.0),
            Value::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "on" => Some(true),
                "false" | "0" | "off" => Some(false),
                _ => None,
            },
            Value::Null => None,
        }
    }

    /// Converte un valore generico (es. da JSON/HMI/script) nel tipo del tag.
    pub fn coerce(&self, dt: DataType) -> anyhow::Result<Value> {
        let err = || anyhow::anyhow!("impossibile convertire {self:?} in {dt:?}");
        Ok(match dt {
            DataType::Bool => Value::Bool(self.as_bool().ok_or_else(err)?),
            DataType::F32 => Value::Float(self.as_f64().ok_or_else(err)?),
            DataType::String => Value::Str(self.to_string()),
            _ => {
                let v = self.as_i64().ok_or_else(err)?;
                let (min, max) = int_range(dt);
                if v < min || v > max {
                    anyhow::bail!("valore {v} fuori range per {dt:?}");
                }
                Value::Int(v)
            }
        })
    }

    /// Decodifica big-endian (ordine Siemens / Modbus standard) da un buffer.
    pub fn decode_be(dt: DataType, buf: &[u8], bit: u8) -> anyhow::Result<Value> {
        let need = dt.byte_size(buf.len());
        anyhow::ensure!(buf.len() >= need, "buffer troppo corto per {dt:?}");
        Ok(match dt {
            DataType::Bool => Value::Bool(buf[0] >> bit & 1 == 1),
            DataType::U8 => Value::Int(buf[0] as i64),
            DataType::I16 => Value::Int(i16::from_be_bytes([buf[0], buf[1]]) as i64),
            DataType::U16 => Value::Int(u16::from_be_bytes([buf[0], buf[1]]) as i64),
            DataType::I32 => Value::Int(i32::from_be_bytes(buf[..4].try_into()?) as i64),
            DataType::U32 => Value::Int(u32::from_be_bytes(buf[..4].try_into()?) as i64),
            DataType::F32 => Value::Float(f32::from_be_bytes(buf[..4].try_into()?) as f64),
            DataType::String => {
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                Value::Str(String::from_utf8_lossy(&buf[..end]).trim_end().to_string())
            }
        })
    }

    /// Codifica big-endian. Per le stringhe produce `len` byte (padding con 0).
    pub fn encode_be(&self, dt: DataType, len: usize) -> anyhow::Result<Vec<u8>> {
        let v = self.coerce(dt)?;
        Ok(match (dt, &v) {
            (DataType::Bool, Value::Bool(b)) => vec![*b as u8],
            (DataType::U8, Value::Int(i)) => vec![*i as u8],
            (DataType::I16, Value::Int(i)) => (*i as i16).to_be_bytes().to_vec(),
            (DataType::U16, Value::Int(i)) => (*i as u16).to_be_bytes().to_vec(),
            (DataType::I32, Value::Int(i)) => (*i as i32).to_be_bytes().to_vec(),
            (DataType::U32, Value::Int(i)) => (*i as u32).to_be_bytes().to_vec(),
            (DataType::F32, Value::Float(x)) => (*x as f32).to_be_bytes().to_vec(),
            (DataType::String, Value::Str(s)) => {
                let mut b = s.as_bytes().to_vec();
                b.resize(len, 0);
                b
            }
            _ => anyhow::bail!("codifica non supportata {dt:?} {v:?}"),
        })
    }
}

fn int_range(dt: DataType) -> (i64, i64) {
    match dt {
        DataType::U8 => (0, u8::MAX as i64),
        DataType::I16 => (i16::MIN as i64, i16::MAX as i64),
        DataType::U16 => (0, u16::MAX as i64),
        DataType::I32 => (i32::MIN as i64, i32::MAX as i64),
        DataType::U32 => (0, u32::MAX as i64),
        _ => (i64::MIN, i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_be() {
        for (dt, v) in [
            (DataType::I16, Value::Int(-1234)),
            (DataType::U32, Value::Int(4_000_000_000)),
            (DataType::F32, Value::Float(12.5)),
            (DataType::String, Value::Str("ABC123".into())),
        ] {
            let b = v.encode_be(dt, 10).unwrap();
            assert_eq!(Value::decode_be(dt, &b, 0).unwrap(), v);
        }
        assert_eq!(Value::decode_be(DataType::Bool, &[0b0000_0100], 2).unwrap(), Value::Bool(true));
    }

    #[test]
    fn coerce_range() {
        assert!(Value::Int(70000).coerce(DataType::U16).is_err());
        assert_eq!(Value::Str("on".into()).coerce(DataType::Bool).unwrap(), Value::Bool(true));
    }
}
