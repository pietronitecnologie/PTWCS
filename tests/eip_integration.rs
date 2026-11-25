//! Test driver EtherNet/IP contro il simulatore Logix di cpppo.
//! Avvio: python -m cpppo.server.enip --address 127.0.0.1:44818 ReqID=DINT Run=BOOL Temp=REAL Cnt=INT
//! Esecuzione: EIP_TEST_HOST=127.0.0.1 cargo test --features eip --test eip_integration
#![cfg(feature = "eip")]
use wcs_scada::config::{DriverConfig, TagDef};
use wcs_scada::drivers::{Driver, eip::EipDriver};
use wcs_scada::value::Value;

fn tag(name: &str, ty: &str) -> TagDef {
    serde_yaml::from_str(&format!("{{name: {name}, driver: ab, address: {name}, type: {ty}, writable: true}}")).unwrap()
}

#[tokio::test]
async fn eip_read_write() {
    let Ok(host) = std::env::var("EIP_TEST_HOST") else {
        eprintln!("EIP_TEST_HOST non impostata: test saltato");
        return;
    };
    let cfg: DriverConfig = serde_yaml::from_str(&format!("{{name: ab, protocol: eip, host: '{host}', slot: 0}}")).unwrap();
    let tags = vec![tag("ReqID", "i32"), tag("Run", "bool"), tag("Temp", "f32"), tag("Cnt", "i16")];
    let mut d = EipDriver::new(&cfg, &tags).unwrap();
    d.connect().await.unwrap();
    d.write(&tags[0], &Value::Int(123456)).await.unwrap();
    d.write(&tags[1], &Value::Bool(true)).await.unwrap();
    d.write(&tags[2], &Value::Float(36.5)).await.unwrap();
    d.write(&tags[3], &Value::Int(-7)).await.unwrap();
    let v = d.read(&tags).await.unwrap();
    assert_eq!(v, vec![Value::Int(123456), Value::Bool(true), Value::Float(36.5), Value::Int(-7)]);
    d.disconnect().await;
}
