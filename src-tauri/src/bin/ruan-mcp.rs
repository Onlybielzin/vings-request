// Binario `ruan-mcp` — servidor MCP do ruan sobre STDIO (JSON-RPC 2.0,
// line-delimited). Expoe as tools `ruan_*` para uma IA criar/editar colecoes,
// pastas e requests operando direto nos arquivos .yml, reusando `ruan_lib::store`.
//
// Arquitetura:
// - Este bin so cuida do TRANSPORTE: le linhas do stdin, parseia JSON-RPC,
//   despacha e escreve uma resposta JSON por linha no stdout.
// - A LOGICA das tools vive em `ruan_lib::mcp` (`executar_tool` / `lista_tools`),
//   que e testavel isoladamente (cargo test) sem stdio.
//
// REGRAS DO PROTOCOLO:
// - stdout e EXCLUSIVO do protocolo: uma mensagem JSON-RPC por linha, nada mais.
// - Qualquer log/diagnostico vai para STDERR.
// - Loop sincrono bloqueante (sem tokio).
//
// Metodos tratados:
//   initialize                  -> protocolVersion + capabilities + serverInfo
//   notifications/initialized   -> no-op (notificacao, sem resposta)
//   tools/list                  -> { tools: [...] }
//   tools/call                  -> dispatch; resultado em content[].text (JSON)
//   (qualquer outro com id)     -> erro JSON-RPC -32601 (method not found)

use std::io::{self, BufRead, Read, Write};

use ruan_lib::mcp::{executar_tool, lista_tools, Estado};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "vings-request";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Teto por linha JSON-RPC lida do stdin (16 MiB). O cliente MCP local e
/// confiavel, mas evitar alocacao ilimitada protege contra uma linha gigante
/// (acidental ou maliciosa) que derrubaria o processo por OOM antes do parse.
const MAX_LINHA_BYTES: u64 = 16 * 1024 * 1024;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut estado = Estado::novo();

    eprintln!("[ruan-mcp] servidor iniciado (protocol {PROTOCOL_VERSION})");

    // Le linha a linha com teto de bytes (ver MAX_LINHA_BYTES).
    let mut reader = stdin.lock();
    let mut buf = String::new();
    loop {
        buf.clear();
        let lido = {
            let mut limitado = (&mut reader).take(MAX_LINHA_BYTES);
            match limitado.read_line(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("[ruan-mcp] erro lendo stdin: {e}");
                    break;
                }
            }
        };
        if lido == 0 {
            break; // EOF
        }
        // Se atingiu o teto sem newline, a linha foi truncada: descarta com erro.
        if lido as u64 >= MAX_LINHA_BYTES && !buf.ends_with('\n') {
            eprintln!("[ruan-mcp] linha excede {MAX_LINHA_BYTES} bytes; descartada");
            let resp = erro_jsonrpc(Value::Null, -32700, "Parse error: linha excede o limite");
            escrever(&mut out, &resp);
            continue;
        }
        let linha = buf.trim();
        if linha.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(linha) {
            Ok(v) => v,
            Err(e) => {
                // JSON invalido: responde parse error (-32700) sem id conhecido.
                eprintln!("[ruan-mcp] JSON invalido: {e}");
                let resp = erro_jsonrpc(Value::Null, -32700, "Parse error");
                escrever(&mut out, &resp);
                continue;
            }
        };

        // Notificacao (sem `id`) -> nunca responde. So tratamos as conhecidas.
        let id = req.get("id").cloned();
        let metodo = req.get("method").and_then(Value::as_str).unwrap_or("");

        if id.is_none() {
            // notifications/initialized e afins: no-op silencioso.
            if metodo != "notifications/initialized" {
                eprintln!("[ruan-mcp] notificacao ignorada: {metodo}");
            }
            continue;
        }
        let id = id.unwrap();

        let resp = match metodo {
            "initialize" => sucesso(id, resultado_initialize()),
            "tools/list" => sucesso(id, json!({ "tools": lista_tools() })),
            "tools/call" => tratar_tools_call(id, &req, &mut estado),
            outro => {
                eprintln!("[ruan-mcp] metodo desconhecido: {outro}");
                erro_jsonrpc(id, -32601, "Method not found")
            }
        };
        escrever(&mut out, &resp);
    }

    eprintln!("[ruan-mcp] stdin encerrado, saindo");
}

/// Resultado do `initialize`.
fn resultado_initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
    })
}

/// Trata `tools/call`: extrai name+arguments, executa e empacota o resultado.
/// Erros de tool NAO viram erro JSON-RPC: viram resultado com isError:true
/// (convencao MCP, para a IA ver a mensagem).
fn tratar_tools_call(id: Value, req: &Value, estado: &mut Estado) -> Value {
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let nome = match params.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None => {
            return erro_jsonrpc(id, -32602, "Invalid params: 'name' ausente");
        }
    };
    // arguments e opcional; default objeto vazio.
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match executar_tool(&nome, &args, estado) {
        Ok(valor) => {
            let texto = serde_json::to_string_pretty(&valor)
                .unwrap_or_else(|_| valor.to_string());
            sucesso(
                id,
                json!({
                    "content": [{ "type": "text", "text": texto }],
                    "isError": false
                }),
            )
        }
        Err(msg) => {
            eprintln!("[ruan-mcp] tool '{nome}' falhou: {msg}");
            sucesso(
                id,
                json!({
                    "content": [{ "type": "text", "text": msg }],
                    "isError": true
                }),
            )
        }
    }
}

/// Monta uma resposta JSON-RPC de sucesso.
fn sucesso(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Monta uma resposta JSON-RPC de erro.
fn erro_jsonrpc(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Escreve uma mensagem JSON-RPC no stdout, uma por linha, e da flush.
fn escrever<W: Write>(out: &mut W, msg: &Value) {
    match serde_json::to_string(msg) {
        Ok(s) => {
            if writeln!(out, "{s}").is_err() {
                eprintln!("[ruan-mcp] falha ao escrever no stdout");
                return;
            }
            let _ = out.flush();
        }
        Err(e) => eprintln!("[ruan-mcp] falha ao serializar resposta: {e}"),
    }
}
