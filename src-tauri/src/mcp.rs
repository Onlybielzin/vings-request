// Servidor MCP do ruan — LOGICA PURA de dispatch das tools.
//
// Este modulo NAO faz I/O de stdio nem fala JSON-RPC: ele apenas recebe o nome
// de uma tool + os argumentos (ja parseados como `serde_json::Value`) e executa
// a operacao correspondente reusando `crate::store`. O loop stdio/JSON-RPC fica
// no binario `src/bin/ruan-mcp.rs`, que so chama `executar_tool` e `lista_tools`.
//
// Toda operacao de disco REUSA a store (slug_seguro + dentro_de garantem que
// nada escapa do diretorio da colecao). Aqui NAO reimplementamos seguranca de
// path: os caminhos `dir`/`collectionPath` apenas viram `PathBuf` e descem para
// as primitivas seguras da store, que validam.
//
// `executar_tool(nome, args, &mut Estado) -> Result<serde_json::Value, String>`
// e a fronteira testavel: erro = String amigavel (vira isError no protocolo).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::store::collection_ops;
use crate::store::fs_store;
use crate::store::models::{Auth, Body, KeyValue, RequestItem, Scripts};
use crate::store::tree_ops;

/// Estado do servidor entre chamadas. Hoje so guarda a ultima colecao aberta,
/// mas existe para permitir cache/contexto futuro sem mudar a assinatura.
#[derive(Default)]
pub struct Estado {
    /// Caminho da colecao aberta mais recentemente (conveniencia/diagnostico).
    pub ultima_colecao: Option<PathBuf>,
}

impl Estado {
    pub fn novo() -> Self {
        Estado::default()
    }
}

/// Converte um `StoreError` (ou qualquer Display) numa String amigavel.
fn err_str<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Le um campo String obrigatorio de um objeto JSON.
fn campo_str(args: &Value, chave: &str) -> Result<String, String> {
    args.get(chave)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("campo obrigatorio ausente ou nao-string: '{chave}'"))
}

/// Le um campo String opcional.
fn campo_str_opt(args: &Value, chave: &str) -> Option<String> {
    args.get(chave).and_then(Value::as_str).map(str::to_string)
}

/// Le um `dir` opcional (subcaminho relativo dentro da colecao). Ausente => raiz.
fn campo_dir(args: &Value) -> Option<String> {
    campo_str_opt(args, "dir")
}

/// Le um `seq` opcional (u32). Ausente => 0.
fn campo_seq(args: &Value) -> u32 {
    args.get("seq")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(0)
}

/// Resolve `collectionPath` + `dir` opcional num diretorio absoluto dentro da
/// colecao. None => raiz da colecao.
///
/// SEGURANCA: `dir` e input nao-confiavel da IA. NAO confiamos no `starts_with`
/// lexico de `dentro_de` (que pode ser burlado por `..` reanexado a um sufixo
/// inexistente, ex.: "naoexiste/../.."). Em vez disso, validamos COMPONENTE A
/// COMPONENTE com `slug_seguro` (rejeita `..`, `.`, `/`, `\`, NUL, absolutos)
/// antes de juntar. Isso tambem alinha o `dir` ao layout real em disco, onde as
/// pastas sao gravadas como slug. `dentro_de` permanece como defesa em
/// profundidade no final.
fn resolver(collection_dir: &Path, dir: Option<String>) -> Result<PathBuf, String> {
    let mut target = collection_dir.to_path_buf();
    if let Some(d) = dir {
        for comp in d.split(['/', '\\']) {
            // Tolera "a//b", barra inicial/final e dir vazio.
            if comp.is_empty() {
                continue;
            }
            let slug = crate::store::slug::slug_seguro(comp).map_err(err_str)?;
            target.push(slug);
        }
    }
    // Defesa em profundidade: confirma que o alvo nao escapa da colecao.
    fs_store::dentro_de(collection_dir, &target).map_err(err_str)?;
    Ok(target)
}

/// Dispatch central: nome da tool + args -> resultado JSON (ou erro String).
///
/// Tools (prefixo `ruan_`):
/// - ruan_open_collection     { path }
/// - ruan_create_collection   { parentDir, name }
/// - ruan_create_folder       { collectionPath, dir?, name, seq? }
/// - ruan_create_request      { collectionPath, dir?, name, method?, url?,
///                              headers?, params?, body?, auth?, seq? }
/// - ruan_update_request      { collectionPath, dir?, name, patch }
/// - ruan_rename_item         { collectionPath, dir?, kind, oldName, newName }
/// - ruan_duplicate_item      { collectionPath, dir?, name, newName?, seq? }
/// - ruan_move_item           { collectionPath, kind, fromDir?, toDir?, name, newSeq? }
/// - ruan_delete_request      { collectionPath, dir?, name }
pub fn executar_tool(
    nome: &str,
    args: &Value,
    estado: &mut Estado,
) -> Result<Value, String> {
    match nome {
        "ruan_open_collection" => {
            let path = campo_str(args, "path")?;
            let dir = PathBuf::from(&path);
            let col = fs_store::load_collection(&dir).map_err(err_str)?;
            estado.ultima_colecao = Some(dir);
            serde_json::to_value(&col).map_err(err_str)
        }

        "ruan_create_collection" => {
            let parent = campo_str(args, "parentDir")?;
            let name = campo_str(args, "name")?;
            let (path, col) =
                collection_ops::create_collection_at(parent, name).map_err(err_str)?;
            estado.ultima_colecao = Some(path.clone());
            Ok(json!({
                "path": path.display().to_string(),
                "collection": serde_json::to_value(&col).map_err(err_str)?,
            }))
        }

        "ruan_create_folder" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let name = campo_str(args, "name")?;
            let seq = campo_seq(args);
            let written =
                fs_store::create_folder(&collection_dir, &target, &name, seq).map_err(err_str)?;
            Ok(json!({ "path": written.display().to_string() }))
        }

        "ruan_create_request" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let name = campo_str(args, "name")?;
            let seq = campo_seq(args);
            // Comeca de uma request default e aplica os campos opcionais fornecidos.
            let mut req = tree_ops::request_default(&name, seq);
            aplicar_patch(&mut req, args)?;
            let written =
                fs_store::save_request(&collection_dir, &target, &req).map_err(err_str)?;
            Ok(json!({ "path": written.display().to_string() }))
        }

        "ruan_update_request" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let name = campo_str(args, "name")?;
            let patch = args
                .get("patch")
                .ok_or_else(|| "campo obrigatorio ausente: 'patch'".to_string())?;
            // Carrega a request existente do disco pelo slug do nome atual.
            let mut req = carregar_request(&target, &name)?;
            let slug_antigo = crate::store::slug::slug_seguro(&req.name).map_err(err_str)?;
            // Mescla os campos do patch (incl. possivel rename via patch.name).
            aplicar_patch(&mut req, patch)?;
            let written =
                fs_store::save_request(&collection_dir, &target, &req).map_err(err_str)?;
            // Se o patch renomeou (slug mudou), remove o arquivo antigo.
            let slug_novo = crate::store::slug::slug_seguro(&req.name).map_err(err_str)?;
            if slug_novo != slug_antigo {
                let antigo = target.join(format!("{slug_antigo}.yml"));
                let _ = std::fs::remove_file(antigo);
            }
            Ok(json!({
                "path": written.display().to_string(),
                "request": serde_json::to_value(&req).map_err(err_str)?,
            }))
        }

        "ruan_rename_item" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let kind = campo_str(args, "kind")?;
            let old_name = campo_str(args, "oldName")?;
            let new_name = campo_str(args, "newName")?;
            let written = match kind.as_str() {
                "folder" => tree_ops::rename_folder(&collection_dir, &target, &old_name, &new_name),
                "request" => {
                    tree_ops::rename_request(&collection_dir, &target, &old_name, &new_name)
                }
                outro => return Err(format!("kind invalido: '{outro}' (use 'folder' ou 'request')")),
            }
            .map_err(err_str)?;
            Ok(json!({ "path": written.display().to_string() }))
        }

        "ruan_duplicate_item" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let name = campo_str(args, "name")?;
            // newName default: "<name> copia" (espelha a convencao do front).
            let new_name =
                campo_str_opt(args, "newName").unwrap_or_else(|| format!("{name} copia"));
            let seq = campo_seq(args);
            let written =
                tree_ops::duplicate_request(&collection_dir, &target, &name, &new_name, seq)
                    .map_err(err_str)?;
            Ok(json!({ "path": written.display().to_string() }))
        }

        "ruan_move_item" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let from = resolver(&collection_dir, campo_str_opt(args, "fromDir"))?;
            let to = resolver(&collection_dir, campo_str_opt(args, "toDir"))?;
            let kind = campo_str(args, "kind")?;
            let name = campo_str(args, "name")?;
            let new_seq = args
                .get("newSeq")
                .and_then(Value::as_u64)
                .map(|n| n as u32)
                .unwrap_or(0);
            let written = match kind.as_str() {
                "folder" => tree_ops::move_folder(&collection_dir, &from, &to, &name, new_seq),
                "request" => tree_ops::move_request(&collection_dir, &from, &to, &name, new_seq),
                outro => return Err(format!("kind invalido: '{outro}' (use 'folder' ou 'request')")),
            }
            .map_err(err_str)?;
            Ok(json!({ "path": written.display().to_string() }))
        }

        "ruan_delete_request" => {
            let collection_path = campo_str(args, "collectionPath")?;
            let collection_dir = PathBuf::from(&collection_path);
            let target = resolver(&collection_dir, campo_dir(args))?;
            let name = campo_str(args, "name")?;
            fs_store::delete_request(&collection_dir, &target, &name).map_err(err_str)?;
            Ok(json!({ "deleted": name }))
        }

        outro => Err(format!("tool desconhecida: '{outro}'")),
    }
}

/// Carrega uma `RequestItem` do disco pelo nome (slug). `dir` ja resolvido.
fn carregar_request(dir: &Path, name: &str) -> Result<RequestItem, String> {
    let slug = crate::store::slug::slug_seguro(name).map_err(err_str)?;
    let alvo = dir.join(format!("{slug}.yml"));
    let yaml = std::fs::read_to_string(&alvo)
        .map_err(|e| format!("request inexistente ou ilegivel ({}): {e}", alvo.display()))?;
    crate::store::parser::parse_request(&yaml).map_err(err_str)
}

/// Aplica um patch parcial (objeto JSON) sobre uma `RequestItem`. Apenas os
/// campos presentes no patch sao alterados; os demais ficam intactos.
///
/// Campos suportados: name, seq, method, url, headers, params, body, auth,
/// scripts, tests, docs. Cada bloco estruturado (headers/params/body/auth/
/// scripts) e desserializado pelos proprios tipos serde da store (camelCase),
/// entao um patch malformado vira erro de parse, nunca corrompe a request.
fn aplicar_patch(req: &mut RequestItem, patch: &Value) -> Result<(), String> {
    let obj = match patch.as_object() {
        Some(o) => o,
        None => {
            // Patch nao-objeto e aceito apenas se vazio/null (no-op).
            if patch.is_null() {
                return Ok(());
            }
            return Err("patch deve ser um objeto JSON".to_string());
        }
    };

    if let Some(v) = obj.get("name").and_then(Value::as_str) {
        req.name = v.to_string();
    }
    if let Some(v) = obj.get("seq").and_then(Value::as_u64) {
        req.seq = v as u32;
    }
    if let Some(v) = obj.get("method").and_then(Value::as_str) {
        req.method = v.to_string();
    }
    if let Some(v) = obj.get("url").and_then(Value::as_str) {
        req.url = v.to_string();
    }
    if let Some(v) = obj.get("headers") {
        req.headers = parse_campo::<Vec<KeyValue>>(v, "headers")?;
    }
    if let Some(v) = obj.get("params") {
        req.params = parse_campo::<Vec<KeyValue>>(v, "params")?;
    }
    if let Some(v) = obj.get("body") {
        req.body = parse_campo::<Body>(v, "body")?;
    }
    if let Some(v) = obj.get("auth") {
        req.auth = parse_campo::<Auth>(v, "auth")?;
    }
    if let Some(v) = obj.get("scripts") {
        req.scripts = parse_campo::<Scripts>(v, "scripts")?;
    }
    if let Some(v) = obj.get("tests").and_then(Value::as_str) {
        req.tests = v.to_string();
    }
    if let Some(v) = obj.get("docs").and_then(Value::as_str) {
        req.docs = v.to_string();
    }
    Ok(())
}

/// Desserializa um sub-bloco JSON do patch para um tipo serde da store,
/// transformando erro num texto amigavel que cita o campo.
fn parse_campo<T: serde::de::DeserializeOwned>(v: &Value, campo: &str) -> Result<T, String> {
    serde_json::from_value(v.clone())
        .map_err(|e| format!("campo '{campo}' invalido: {e}"))
}

/// Lista das tools com seus inputSchema (JSON Schema). Consumida por `tools/list`.
pub fn lista_tools() -> Value {
    // Sub-schemas reutilizados.
    let key_value = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "value": { "type": "string" },
            "enabled": { "type": "boolean" },
            "description": { "type": "string" }
        },
        "required": ["name"]
    });
    let kv_array = json!({ "type": "array", "items": key_value });
    let body_schema = json!({
        "type": "object",
        "description": "Corpo da request. 'mode' decide qual payload e relevante.",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["none", "json", "text", "xml", "form_urlencoded", "multipart", "graphql"]
            },
            "raw": { "type": "string", "description": "Texto cru para json/text/xml." },
            "form": kv_array,
            "graphql": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "variables": { "type": "string", "description": "JSON como string." }
                }
            }
        }
    });
    let auth_schema = json!({
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["none", "inherit", "basic", "bearer", "apikey", "oauth2"]
            },
            "username": { "type": "string" },
            "password": { "type": "string" },
            "token": { "type": "string" },
            "key": { "type": "string" },
            "value": { "type": "string" },
            "placement": { "type": "string", "enum": ["header", "query"] }
        }
    });
    let scripts_schema = json!({
        "type": "object",
        "properties": {
            "pre": { "type": "string" },
            "post": { "type": "string" }
        }
    });

    json!([
        {
            "name": "ruan_open_collection",
            "description": "Abre uma colecao do disco e devolve a arvore completa (pastas e requests com seus campos).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho absoluto da pasta da colecao (contem collection.yml)." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "ruan_create_collection",
            "description": "Cria uma colecao nova em <parentDir>/<slug(name)>/ com collection.yml inicial.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parentDir": { "type": "string", "description": "Diretorio-pai onde a pasta da colecao sera criada." },
                    "name": { "type": "string", "description": "Nome da colecao (vira slug no nome da pasta)." }
                },
                "required": ["parentDir", "name"]
            }
        },
        {
            "name": "ruan_create_folder",
            "description": "Cria uma subpasta dentro da colecao (com folder.yml).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string", "description": "Caminho absoluto da colecao." },
                    "dir": { "type": "string", "description": "Subcaminho relativo (slugs unidos por '/'); ausente = raiz." },
                    "name": { "type": "string" },
                    "seq": { "type": "integer", "minimum": 0, "description": "Ordem de exibicao (default 0)." }
                },
                "required": ["collectionPath", "name"]
            }
        },
        {
            "name": "ruan_create_request",
            "description": "Cria uma request nova. Sem method/url comeca como GET vazia; campos opcionais sao aplicados na criacao.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "dir": { "type": "string", "description": "Subcaminho relativo; ausente = raiz." },
                    "name": { "type": "string" },
                    "method": { "type": "string", "description": "GET/POST/PUT/PATCH/DELETE/... (default GET)." },
                    "url": { "type": "string" },
                    "headers": kv_array,
                    "params": kv_array,
                    "body": body_schema,
                    "auth": auth_schema,
                    "seq": { "type": "integer", "minimum": 0 }
                },
                "required": ["collectionPath", "name"]
            }
        },
        {
            "name": "ruan_update_request",
            "description": "Aplica um patch parcial numa request existente (carrega do disco, mescla, regrava). Campos do patch: name, seq, method, url, headers, params, body, auth, scripts, tests, docs. Renomear via patch.name move o arquivo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "dir": { "type": "string", "description": "Subcaminho relativo; ausente = raiz." },
                    "name": { "type": "string", "description": "Nome atual da request a editar." },
                    "patch": {
                        "type": "object",
                        "description": "Campos a sobrescrever. Apenas os presentes mudam.",
                        "properties": {
                            "name": { "type": "string" },
                            "seq": { "type": "integer", "minimum": 0 },
                            "method": { "type": "string" },
                            "url": { "type": "string" },
                            "headers": kv_array,
                            "params": kv_array,
                            "body": body_schema,
                            "auth": auth_schema,
                            "scripts": scripts_schema,
                            "tests": { "type": "string" },
                            "docs": { "type": "string" }
                        }
                    }
                },
                "required": ["collectionPath", "name", "patch"]
            }
        },
        {
            "name": "ruan_rename_item",
            "description": "Renomeia uma request ou pasta dentro de um diretorio da colecao.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "dir": { "type": "string", "description": "Subcaminho relativo; ausente = raiz." },
                    "kind": { "type": "string", "enum": ["folder", "request"] },
                    "oldName": { "type": "string" },
                    "newName": { "type": "string" }
                },
                "required": ["collectionPath", "kind", "oldName", "newName"]
            }
        },
        {
            "name": "ruan_duplicate_item",
            "description": "Duplica uma request dentro de um diretorio. newName default = '<name> copia'.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "dir": { "type": "string", "description": "Subcaminho relativo; ausente = raiz." },
                    "name": { "type": "string" },
                    "newName": { "type": "string" },
                    "seq": { "type": "integer", "minimum": 0 }
                },
                "required": ["collectionPath", "name"]
            }
        },
        {
            "name": "ruan_move_item",
            "description": "Move/reordena uma request ou pasta entre diretorios da colecao.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "kind": { "type": "string", "enum": ["folder", "request"] },
                    "fromDir": { "type": "string", "description": "Origem (relativo); ausente = raiz." },
                    "toDir": { "type": "string", "description": "Destino (relativo); ausente = raiz." },
                    "name": { "type": "string" },
                    "newSeq": { "type": "integer", "minimum": 0 }
                },
                "required": ["collectionPath", "kind", "name"]
            }
        },
        {
            "name": "ruan_delete_request",
            "description": "Remove uma request pelo nome dentro de um diretorio da colecao (idempotente).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "collectionPath": { "type": "string" },
                    "dir": { "type": "string", "description": "Subcaminho relativo; ausente = raiz." },
                    "name": { "type": "string" }
                },
                "required": ["collectionPath", "name"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fs_store::load_collection;
    use crate::store::models::{AuthMode, BodyMode, TreeItem};
    use tempfile::TempDir;

    fn col_temp() -> (TempDir, String) {
        let td = TempDir::new().unwrap();
        let (path, _col) = collection_ops::create_collection_at(
            td.path().display().to_string(),
            "Minha API".to_string(),
        )
        .unwrap();
        (td, path.display().to_string())
    }

    #[test]
    fn open_collection_devolve_arvore() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        let r = executar_tool("ruan_open_collection", &json!({ "path": col }), &mut st).unwrap();
        assert_eq!(r["name"], "Minha API");
        assert!(r["items"].is_array());
        assert!(st.ultima_colecao.is_some());
    }

    #[test]
    fn create_collection_via_tool() {
        let td = TempDir::new().unwrap();
        let mut st = Estado::novo();
        let r = executar_tool(
            "ruan_create_collection",
            &json!({ "parentDir": td.path().display().to_string(), "name": "Nova" }),
            &mut st,
        )
        .unwrap();
        let path = r["path"].as_str().unwrap();
        assert!(Path::new(path).join("collection.yml").is_file());
    }

    #[test]
    fn create_request_com_campos_e_le_de_volta() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        executar_tool(
            "ruan_create_request",
            &json!({
                "collectionPath": col,
                "name": "Listar",
                "method": "POST",
                "url": "https://x/y",
                "headers": [{ "name": "X-A", "value": "1" }],
                "body": { "mode": "json", "raw": "{}" },
                "auth": { "mode": "bearer", "token": "tkn" }
            }),
            &mut st,
        )
        .unwrap();
        let c = load_collection(Path::new(&col)).unwrap();
        match &c.items[0] {
            TreeItem::Request(rq) => {
                assert_eq!(rq.method, "POST");
                assert_eq!(rq.url, "https://x/y");
                assert_eq!(rq.headers.len(), 1);
                assert_eq!(rq.body.mode, BodyMode::Json);
                assert_eq!(rq.auth.mode, AuthMode::Bearer);
            }
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn update_request_patch_parcial_preserva_resto() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        executar_tool(
            "ruan_create_request",
            &json!({ "collectionPath": col, "name": "Req", "method": "GET", "url": "http://a" }),
            &mut st,
        )
        .unwrap();
        // Patch so o method; url deve permanecer.
        executar_tool(
            "ruan_update_request",
            &json!({ "collectionPath": col, "name": "Req", "patch": { "method": "DELETE" } }),
            &mut st,
        )
        .unwrap();
        let c = load_collection(Path::new(&col)).unwrap();
        match &c.items[0] {
            TreeItem::Request(rq) => {
                assert_eq!(rq.method, "DELETE");
                assert_eq!(rq.url, "http://a");
            }
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn update_request_rename_via_patch_move_arquivo() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        executar_tool(
            "ruan_create_request",
            &json!({ "collectionPath": col, "name": "Antigo" }),
            &mut st,
        )
        .unwrap();
        executar_tool(
            "ruan_update_request",
            &json!({ "collectionPath": col, "name": "Antigo", "patch": { "name": "Novo" } }),
            &mut st,
        )
        .unwrap();
        assert!(!Path::new(&col).join("antigo.yml").exists());
        assert!(Path::new(&col).join("novo.yml").is_file());
    }

    #[test]
    fn delete_request_via_tool() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        executar_tool(
            "ruan_create_request",
            &json!({ "collectionPath": col, "name": "Temp" }),
            &mut st,
        )
        .unwrap();
        executar_tool(
            "ruan_delete_request",
            &json!({ "collectionPath": col, "name": "Temp" }),
            &mut st,
        )
        .unwrap();
        let c = load_collection(Path::new(&col)).unwrap();
        assert!(c.items.is_empty());
    }

    #[test]
    fn nome_malicioso_e_rejeitado() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        let r = executar_tool(
            "ruan_create_request",
            &json!({ "collectionPath": col, "name": "../escapa" }),
            &mut st,
        );
        assert!(r.is_err());
    }

    #[test]
    fn dir_fora_da_colecao_e_rejeitado() {
        let (_td, col) = col_temp();
        let mut st = Estado::novo();
        let r = executar_tool(
            "ruan_create_request",
            &json!({ "collectionPath": col, "dir": "../fora", "name": "x" }),
            &mut st,
        );
        assert!(r.is_err());
    }

    #[test]
    fn tool_desconhecida_erra() {
        let mut st = Estado::novo();
        assert!(executar_tool("ruan_nada", &json!({}), &mut st).is_err());
    }

    #[test]
    fn campo_obrigatorio_ausente_erra() {
        let mut st = Estado::novo();
        assert!(executar_tool("ruan_open_collection", &json!({}), &mut st).is_err());
    }

    #[test]
    fn lista_tools_tem_nove_tools_com_prefixo() {
        let tools = lista_tools();
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 9);
        for t in arr {
            let nome = t["name"].as_str().unwrap();
            assert!(nome.starts_with("ruan_"), "tool sem prefixo: {nome}");
            assert!(t["inputSchema"].is_object());
        }
    }
}
