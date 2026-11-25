//! Test driver OPC UA contro server asyncua.
//! Avvio: python tools/opcua_test_server.py 4841
//! Esecuzione: OPCUA_TEST_PORT=4841 cargo test --features opcua --test opcua_integration
#![cfg(feature = "opcua")]
use wcs_scada::config::{DriverConfig, TagDef};
use wcs_scada::drivers::{Driver, opcua::OpcUaDriver};
use wcs_scada::value::Value;

fn tag(name: &str, addr: &str, ty: &str) -> TagDef {
    serde_yaml::from_str(&format!("{{name: {name}, driver: ua, address: 'ns=2;s=WCS.{addr}', type: {ty}, writable: true}}")).unwrap()
}

#[tokio::test]
async fn opcua_read_write() {
    let Ok(port) = std::env::var("OPCUA_TEST_PORT") else {
        eprintln!("OPCUA_TEST_PORT non impostata: test saltato");
        return;
    };
    let cfg: DriverConfig =
        serde_yaml::from_str(&format!("{{name: ua, protocol: opcua, endpoint: 'opc.tcp://127.0.0.1:{port}/'}}")).unwrap();
    let tags = vec![
        tag("req", "ReqID", "u16"),
        tag("bc", "Barcode", "string"),
        tag("run", "Run", "bool"),
        tag("t", "Temp", "f32"),
        tag("c", "Count", "i32"),
    ];
    let mut d = OpcUaDriver::new(&cfg, &tags).unwrap();
    d.connect().await.unwrap();
    let v = d.read(&tags).await.unwrap();
    assert_eq!(v, vec![Value::Int(7), Value::Str("PKG-OPC-01".into()), Value::Bool(true), Value::Float(21.5), Value::Int(-42)]);
    d.write(&tags[0], &Value::Int(8)).await.unwrap();
    d.write(&tags[1], &Value::Str("NEW".into())).await.unwrap();
    d.write(&tags[2], &Value::Bool(false)).await.unwrap();
    let v = d.read(&tags).await.unwrap();
    assert_eq!(&v[..3], &[Value::Int(8), Value::Str("NEW".into()), Value::Bool(false)]);
    d.write(&tags[0], &Value::Int(7)).await.unwrap();
    d.write(&tags[1], &Value::Str("PKG-OPC-01".into())).await.unwrap();
    d.write(&tags[2], &Value::Bool(true)).await.unwrap();
    d.disconnect().await;
}
