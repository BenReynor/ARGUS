//! Prueba de humo: lanza `argus-mcp`, hace `initialize` + `tools/list` por stdio
//! y comprueba que aparecen las tres herramientas.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn argus_mcp_lista_las_tres_herramientas_por_stdio() {
    let dir = std::env::temp_dir().join(format!("argus-mcp-humo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut hijo = Command::new(env!("CARGO_BIN_EXE_argus-mcp"))
        .env("ARGUS_DB", dir.join("argus.db"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("no se pudo lanzar argus-mcp");
    let mut entrada = hijo.stdin.take().unwrap();
    let mut salida = BufReader::new(hijo.stdout.take().unwrap());

    let mut enviar = |v: serde_json::Value| {
        writeln!(entrada, "{v}").unwrap();
        entrada.flush().unwrap();
    };
    let mut leer_respuesta = |id: u64| -> serde_json::Value {
        loop {
            let mut linea = String::new();
            assert!(salida.read_line(&mut linea).unwrap() > 0, "stdout cerrado");
            let v: serde_json::Value = serde_json::from_str(linea.trim()).expect("JSON-RPC");
            if v["id"] == id {
                return v;
            }
        }
    };

    enviar(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "humo", "version": "0"}
        }
    }));
    let init = leer_respuesta(1);
    assert!(init["result"]["capabilities"]["tools"].is_object());
    enviar(serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    enviar(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let lista = leer_respuesta(2);
    let nombres: Vec<&str> = lista["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for n in ["estado", "listar_servidores", "elegir_servidor"] {
        assert!(nombres.contains(&n), "falta {n} en {nombres:?}");
    }

    // Sin bot activo la herramienta responde con el aviso, sin caerse.
    enviar(serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "estado", "arguments": {}}
    }));
    let estado = leer_respuesta(3);
    assert!(estado.to_string().contains("No active bot in ARGUS"));

    drop(enviar);
    drop(entrada);
    let _ = hijo.kill();
    let _ = hijo.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
