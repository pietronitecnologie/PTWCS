//! Test di integrazione del driver S7 contro un server snap7.
//! Avviare prima: python tools/s7_test_server.py 1102
//! Eseguire con: S7_TEST_PORT=1102 cargo test --test s7_integration
use wcs_scada::config::{DriverConfig, Protocol, TagDef};
use wcs_scada::drivers::{Driver, s7::S7Driver};
use wcs_scada::value::{DataType, Value};

fn tag(name: &str, addr: &str, dt: DataType) -> TagDef {
    serde_yaml::from_str::<TagDef>(&format!(
        "{{name: {name}, driver: plc, address: '{addr}', type: {}, writable: true}}",
        serde_json::to_value(dt).unwrap().as_str().unwrap()
    ))
    .unwrap()
}

#[tokio::test]
async fn s7_read_write() {
    let Ok(port) = std::env::var("S7_TEST_PORT") else {
        eprintln!("S7_TEST_PORT non impostata: test saltato");
        return;
    };
    let cfg: DriverConfig = serde_yaml::from_str(&format!(
        "{{name: plc, protocol: s7, host: 127.0.0.1, port: {port}, rack: 0, slot: 1}}"
    ))
    .unwrap();
    assert_eq!(cfg.protocol, Protocol::S7);
    let tags = vec![
        tag("b1", "DB10.DBX0.1", DataType::Bool),
        // Bit 0 di un byte libero: il server python-snap7 di test ignora l'offset
        // di bit nelle scritture BIT (una CPU reale no), quindi evitiamo byte condivisi.
        tag("b2", "DB10.DBX1.0", DataType::Bool),
        tag("w", "DB10.DBW2", DataType::I16),
        tag("r", "DB10.DBD4", DataType::F32),
        tag("ud", "DB10.DBD8", DataType::U32),
        tag("s", "DB10.DBB20", DataType::String),
        tag("mw", "MW10", DataType::U16),
        tag("far", "DB10.DBW200", DataType::U16),
    ];
    let mut d = S7Driver::new(&cfg, &tags).unwrap();
    d.connect().await.unwrap();
    let v = d.read(&tags).await.unwrap();
    assert_eq!(v[0], Value::Bool(true));
    assert_eq!(v[1], Value::Bool(false));
    assert_eq!(v[2], Value::Int(-1234));
    assert_eq!(v[3], Value::Float(12.5));
    assert_eq!(v[4], Value::Int(3_000_000_000));
    assert_eq!(v[5], Value::Str("PKG001".into()));
    assert_eq!(v[6], Value::Int(4242));

    d.write(&tags[1], &Value::Bool(true)).await.unwrap();
    d.write(&tags[2], &Value::Int(777)).await.unwrap();
    d.write(&tags[5], &Value::Str("HELLO-WCS".into())).await.unwrap();
    d.write(&tags[7], &Value::Int(65000)).await.unwrap();
    let v = d.read(&tags).await.unwrap();
    assert_eq!(v[0], Value::Bool(true));
    assert_eq!(v[1], Value::Bool(true));
    assert_eq!(v[2], Value::Int(777));
    assert_eq!(v[5], Value::Str("HELLO-WCS".into()));
    assert_eq!(v[7], Value::Int(65000));
    // ripristino
    d.write(&tags[1], &Value::Bool(false)).await.unwrap();
    d.write(&tags[2], &Value::Int(-1234)).await.unwrap();
    d.write(&tags[5], &Value::Str("PKG001".into())).await.unwrap();
}
