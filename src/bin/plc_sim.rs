//! Simulatore PLC Modbus TCP: linea di smistamento colli (nastri, scanner,
//! deviatore con 3 uscite + scarto). Implementa il lato PLC dell'handshake
//! ReqID/RespID con timeout, come farebbe un PLC reale.
//!
//! Mappa Modbus (vedi config/plant.yaml):
//!   coil 0..2  CV1..CV3 marcia (comando/stato)    coil 30  reset guasti
//!   coil 31    inietta guasto CV2 (test)
//!   di 0..2    guasto CV1..CV3   di 3 fotocellula scanner   di 4 emergenza
//!   hr 0       ReqID (PLC)       hr 1 peso [g/10]           hr 2..11 barcode (20 char)
//!   hr 12      RespID (WCS)      hr 13 destinazione (WCS)   hr 14 esito (WCS)
//!   hr 20      posizione deviatore
//!   ir 0..3    contatori uscita 1/2/3/scarto   ir 4 timeout risposta WCS
//!   ir 5       velocità CV1 [m/min*10]   ir 6 corrente motore [A*10]   ir 7 temperatura [°C*10]
//!
//! Uso: plc_sim [bind]   (default 0.0.0.0:5020)

use rand::RngExt;
use std::future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_modbus::prelude::*;
use tokio_modbus::server::tcp::{Server, accept_tcp_connection};

const N: usize = 256;

#[derive(Default)]
struct Mem {
    coils: Vec<bool>,
    di: Vec<bool>,
    hr: Vec<u16>,
    ir: Vec<u16>,
}

#[derive(Clone)]
struct Plc(Arc<Mutex<Mem>>);

fn slice<T: Copy>(v: &[T], a: u16, n: u16) -> Result<Vec<T>, ExceptionCode> {
    let (a, n) = (a as usize, n as usize);
    v.get(a..a + n).map(<[T]>::to_vec).ok_or(ExceptionCode::IllegalDataAddress)
}

impl tokio_modbus::server::Service for Plc {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = future::Ready<Result<Response, ExceptionCode>>;

    fn call(&self, req: Request<'static>) -> Self::Future {
        let mut m = self.0.lock().unwrap();
        let r = (|| -> Result<Response, ExceptionCode> {
            Ok(match req {
                Request::ReadCoils(a, n) => Response::ReadCoils(slice(&m.coils, a, n)?),
                Request::ReadDiscreteInputs(a, n) => Response::ReadDiscreteInputs(slice(&m.di, a, n)?),
                Request::ReadHoldingRegisters(a, n) => Response::ReadHoldingRegisters(slice(&m.hr, a, n)?),
                Request::ReadInputRegisters(a, n) => Response::ReadInputRegisters(slice(&m.ir, a, n)?),
                Request::WriteSingleCoil(a, v) => {
                    *m.coils.get_mut(a as usize).ok_or(ExceptionCode::IllegalDataAddress)? = v;
                    Response::WriteSingleCoil(a, v)
                }
                Request::WriteSingleRegister(a, v) => {
                    *m.hr.get_mut(a as usize).ok_or(ExceptionCode::IllegalDataAddress)? = v;
                    Response::WriteSingleRegister(a, v)
                }
                Request::WriteMultipleRegisters(a, vs) => {
                    let s = a as usize;
                    let dst = m.hr.get_mut(s..s + vs.len()).ok_or(ExceptionCode::IllegalDataAddress)?;
                    dst.copy_from_slice(&vs);
                    Response::WriteMultipleRegisters(a, vs.len() as u16)
                }
                Request::WriteMultipleCoils(a, vs) => {
                    let s = a as usize;
                    let dst = m.coils.get_mut(s..s + vs.len()).ok_or(ExceptionCode::IllegalDataAddress)?;
                    dst.copy_from_slice(&vs);
                    Response::WriteMultipleCoils(a, vs.len() as u16)
                }
                _ => return Err(ExceptionCode::IllegalFunction),
            })
        })();
        future::ready(r)
    }
}

const BARCODES: &[&str] = &[
    "IT-MI-000123", "IT-RM-004512", "IT-TO-778899", "DE-BE-112233", "FR-PA-556677", "IT-NA-990011",
    "ES-MD-445566", "IT-MI-000999", "NOREAD", "IT-BO-123456", "XX-HEAVY-01", "IT-FI-654321",
];

fn put_string(hr: &mut [u16], start: usize, s: &str, chars: usize) {
    let mut b = s.as_bytes().to_vec();
    b.resize(chars, 0);
    for (i, c) in b.chunks(2).enumerate() {
        hr[start + i] = u16::from_be_bytes([c[0], c[1]]);
    }
}

fn simulate(plc: Plc) {
    let mut rng = rand::rng();
    let mut next_pkg = Instant::now() + Duration::from_secs(2);
    let mut pending: Option<(u16, Instant)> = None;
    let mut req_id: u16 = 0;
    let mut temp = 250.0f64;
    loop {
        std::thread::sleep(Duration::from_millis(50));
        let mut m = plc.0.lock().unwrap();
        let now = Instant::now();

        // reset guasti / iniezione guasto
        if m.coils[30] {
            m.coils[30] = false;
            for i in 0..3 {
                m.di[i] = false;
            }
            println!("[sim] reset guasti");
        }
        if m.coils[31] {
            m.coils[31] = false;
            m.di[1] = true;
        }
        if rng.random_bool(0.0002) {
            m.di[2] = true; // guasto casuale CV3
        }
        // un nastro in guasto si ferma
        for i in 0..3 {
            if m.di[i] {
                m.coils[i] = false;
            }
        }
        let line_ok = m.coils[0] && m.coils[1] && m.coils[2] && !m.di[4];

        // analogici
        let target_speed = if m.coils[0] { 600.0 } else { 0.0 };
        let speed = m.ir[5] as f64 + (target_speed - m.ir[5] as f64) * 0.1;
        m.ir[5] = speed.round() as u16;
        m.ir[6] = if m.coils[0] { (52.0 + rng.random_range(-3.0..3.0)) as u16 } else { 0 };
        let target_temp = if m.coils[0] { 400.0 } else { 250.0 };
        temp += (target_temp - temp) * 0.003 + rng.random_range(-0.5..0.5);
        temp = temp.clamp(200.0, 650.0);
        m.ir[7] = temp as u16;

        // arrivo collo allo scanner
        if line_ok && pending.is_none() && now >= next_pkg {
            req_id = req_id.checked_add(1).unwrap_or(1);
            let bc = BARCODES[rng.random_range(0..BARCODES.len())];
            let weight: u16 = if bc.contains("HEAVY") { 3500 } else { rng.random_range(5..2500) };
            m.di[3] = true;
            m.hr[1] = weight;
            put_string(&mut m.hr, 2, bc, 20);
            m.hr[0] = req_id; // ReqID per ultimo, dopo i dati
            pending = Some((req_id, now));
        }
        // risposta del WCS
        if let Some((id, t0)) = pending {
            if m.hr[12] == id {
                let dest = m.hr[13];
                let idx = match dest {
                    1..=3 => dest as usize - 1,
                    _ => 3,
                };
                m.ir[idx] = m.ir[idx].wrapping_add(1);
                m.hr[20] = dest;
                m.di[3] = false;
                pending = None;
                next_pkg = now + Duration::from_millis(rng.random_range(1200..3000));
            } else if now - t0 > Duration::from_secs(2) {
                // timeout: il PLC manda il collo allo scarto (fail-safe)
                m.ir[3] = m.ir[3].wrapping_add(1);
                m.ir[4] = m.ir[4].wrapping_add(1);
                m.hr[20] = 99;
                m.di[3] = false;
                pending = None;
                next_pkg = now + Duration::from_secs(2);
                println!("[sim] timeout risposta WCS per req {id}: scarto");
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind: SocketAddr = std::env::args().nth(1).unwrap_or_else(|| "0.0.0.0:5020".into()).parse()?;
    let mem = Mem { coils: vec![false; N], di: vec![false; N], hr: vec![0; N], ir: vec![0; N] };
    let plc = Plc(Arc::new(Mutex::new(mem)));
    {
        let mut m = plc.0.lock().unwrap();
        m.coils[0] = true;
        m.coils[1] = true;
        m.coils[2] = true;
    }
    let p2 = plc.clone();
    std::thread::spawn(move || simulate(p2));

    let listener = TcpListener::bind(bind).await?;
    println!("[sim] PLC Modbus TCP simulato in ascolto su {bind}");
    let server = Server::new(listener);
    let new_service = move |_addr| Ok(Some(plc.clone()));
    let on_connected = |stream, addr| {
        let ns = new_service.clone();
        async move { accept_tcp_connection(stream, addr, ns) }
    };
    server.serve(&on_connected, |e| eprintln!("[sim] errore: {e}")).await?;
    Ok(())
}
