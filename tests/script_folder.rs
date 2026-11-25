//! Script in cartella: un file per funzione, moduli condivisi `_nome.py`, chiamate di test.
use std::time::Duration;
use wcs_scada::script::ScriptHost;

#[tokio::test]
async fn folder_of_functions_with_shared_module() {
    let dir = std::env::temp_dir().join(format!("wcs_scripts_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("_shared.py"), "stato = {'n': 0}\n").unwrap();
    std::fs::write(dir.join("conta.py"), "from _shared import stato\n\ndef conta(payload, ctx):\n    stato['n'] += 1\n    ctx.write('X', stato['n'])\n    return stato['n']\n").unwrap();
    std::fs::write(dir.join("leggi.py"), "from _shared import stato\n\ndef leggi(payload, ctx):\n    print('debug', payload)\n    return stato['n'] / payload['div']\n").unwrap();
    std::fs::write(dir.join("rotto.py"), "def altro(payload, ctx):\n    return 1\n").unwrap();

    let h = ScriptHost::new("python3".into(), Some(dir.clone()));
    let st = h.start().await.unwrap();
    assert_eq!(st.functions, vec!["conta".to_string(), "leggi".to_string()]);
    assert!(st.load_error.as_deref().unwrap_or("").contains("def rotto"), "il file senza funzione omonima è segnalato");
    assert_eq!(st.files.len(), 4);

    let ctx = serde_json::json!({"tags": {}});
    let r = h.call("conta", serde_json::json!({}), ctx.clone(), Duration::from_secs(5)).await.unwrap();
    assert_eq!(r.result, serde_json::json!(1));
    assert_eq!(r.writes.len(), 1);
    // lo stato del modulo condiviso è lo stesso per tutti gli script
    let t = h.test("leggi", serde_json::json!({"div": 1}), ctx.clone(), Duration::from_secs(5)).await.unwrap();
    assert_eq!(t["ok"], true);
    assert_eq!(t["result"], serde_json::json!(1.0));
    assert_eq!(t["stdout"], "debug {'div': 1}\n");
    assert!(t["duration_ms"].as_f64().is_some());
    // errore con file e riga
    let t = h.test("leggi", serde_json::json!({"div": 0}), ctx, Duration::from_secs(5)).await.unwrap();
    assert_eq!(t["ok"], false);
    assert!(t["error"].as_str().unwrap().starts_with("ZeroDivisionError"));
    let w = t["where"].as_array().unwrap().last().unwrap();
    assert_eq!((w["file"].as_str(), w["line"].as_u64()), (Some("leggi.py"), Some(5)));
    let _ = std::fs::remove_dir_all(&dir);
}
