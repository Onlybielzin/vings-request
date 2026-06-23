// Testes de integracao da LOGICA DE DISPATCH do servidor MCP (`ruan_lib::mcp`).
//
// Estes testes exercitam `executar_tool` / `lista_tools` end-to-end contra um
// tempdir real (tempfile), cobrindo round-trip de disco, patch parcial,
// rename/duplicate/move/delete, SEGURANCA de path (nomes e `dir` maliciosos) e
// o tratamento de erros de dispatch (tool desconhecida / args invalidos).
//
// Ficam em `tests/` (integration target) de proposito: NAO tocam o codigo de
// producao. Linkam contra o crate `ruan_lib` (crate-type rlib) e dependem de
// `pub mod mcp;` estar declarado em `src/lib.rs` — declaracao adicionada pela
// fase de Integracao. Enquanto `mcp` nao for publico, este arquivo nao compila;
// isso e esperado e a suite roda junto com o resto na Integracao.
//
// Cobertura complementa (sem duplicar) os testes unitarios ja existentes em
// `src/mcp.rs::tests`, focando em: round-trip multi-nivel, aninhamento via
// `dir`, traversal NAO trivial (segmento inexistente + `..`), e o contrato de
// erro como `Result::Err` (que o transporte converte em isError).

use ruan_lib::mcp::{executar_tool, lista_tools, Estado};
use ruan_lib::store::fs_store::load_collection;
use ruan_lib::store::models::TreeItem;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------- helpers ----------

/// Cria uma colecao nova num tempdir e devolve (tempdir, caminho_da_colecao).
/// O TempDir e devolvido para manter o diretorio vivo durante o teste.
fn col_temp() -> (TempDir, String) {
    let td = TempDir::new().unwrap();
    let mut st = Estado::novo();
    let r = executar_tool(
        "ruan_create_collection",
        &json!({ "parentDir": td.path().display().to_string(), "name": "API Teste" }),
        &mut st,
    )
    .expect("criar colecao");
    let path = r["path"].as_str().unwrap().to_string();
    (td, path)
}

/// Atalho: executa uma tool com um Estado descartavel e devolve o Result.
fn call(nome: &str, args: Value) -> Result<Value, String> {
    let mut st = Estado::novo();
    executar_tool(nome, &args, &mut st)
}

/// Carrega a colecao do disco para inspecao.
fn carregar(col: &str) -> ruan_lib::store::models::Collection {
    load_collection(Path::new(col)).unwrap()
}

/// Acha uma request por nome na raiz da colecao.
fn req_por_nome<'a>(
    col: &'a ruan_lib::store::models::Collection,
    nome: &str,
) -> Option<&'a ruan_lib::store::models::RequestItem> {
    col.items.iter().find_map(|i| match i {
        TreeItem::Request(r) if r.name == nome => Some(r),
        _ => None,
    })
}

// ---------- round-trip: collection -> folder -> request ----------

#[test]
fn roundtrip_collection_folder_request() {
    let (_td, col) = col_temp();

    // Cria uma pasta na raiz.
    let f = call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "Auth", "seq": 1 }),
    )
    .unwrap();
    assert!(Path::new(f["path"].as_str().unwrap()).join("folder.yml").is_file());

    // Cria uma request DENTRO da pasta (via dir = slug da pasta).
    let r = call(
        "ruan_create_request",
        json!({
            "collectionPath": col,
            "dir": "auth",
            "name": "Login",
            "method": "POST",
            "url": "https://api/login",
            "headers": [{ "name": "Content-Type", "value": "application/json" }],
            "params": [{ "name": "debug", "value": "1" }],
            "body": { "mode": "json", "raw": "{\"u\":1}" },
            "auth": { "mode": "bearer", "token": "abc" },
            "seq": 0
        }),
    )
    .unwrap();
    let req_path = PathBuf::from(r["path"].as_str().unwrap());
    assert_eq!(req_path.file_name().unwrap(), "login.yml");
    assert!(req_path.starts_with(Path::new(&col).join("auth")));

    // open_collection le a arvore inteira de volta.
    let mut st = Estado::novo();
    let tree = executar_tool("ruan_open_collection", &json!({ "path": col }), &mut st).unwrap();
    assert_eq!(tree["name"], "API Teste");
    assert!(st.ultima_colecao.is_some());

    // A pasta Auth existe na arvore e contem Login com todos os campos.
    let col_obj = carregar(&col);
    let folder = col_obj
        .items
        .iter()
        .find_map(|i| match i {
            TreeItem::Folder(fd) if fd.name == "Auth" => Some(fd),
            _ => None,
        })
        .expect("pasta Auth");
    assert_eq!(folder.seq, 1);
    assert_eq!(folder.items.len(), 1);
    match &folder.items[0] {
        TreeItem::Request(rq) => {
            assert_eq!(rq.name, "Login");
            assert_eq!(rq.method, "POST");
            assert_eq!(rq.url, "https://api/login");
            assert_eq!(rq.headers.len(), 1);
            assert_eq!(rq.headers[0].name, "Content-Type");
            assert_eq!(rq.params.len(), 1);
            assert_eq!(rq.params[0].name, "debug");
        }
        _ => panic!("esperava request Login dentro de Auth"),
    }
}

#[test]
fn create_request_na_raiz_default_get() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Ping" }),
    )
    .unwrap();
    let c = carregar(&col);
    let rq = req_por_nome(&c, "Ping").expect("Ping");
    assert_eq!(rq.method, "GET");
    assert_eq!(rq.url, "");
    assert!(rq.headers.is_empty());
}

// ---------- update_request: patch parcial ----------

#[test]
fn update_request_patch_muda_campos_e_preserva_resto() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({
            "collectionPath": col,
            "name": "Editavel",
            "method": "GET",
            "url": "http://orig",
            "headers": [{ "name": "X-Keep", "value": "yes" }]
        }),
    )
    .unwrap();

    // Patch: muda method, url e headers; nada mais informado.
    let upd = call(
        "ruan_update_request",
        json!({
            "collectionPath": col,
            "name": "Editavel",
            "patch": {
                "method": "PUT",
                "url": "http://novo",
                "headers": [{ "name": "X-New", "value": "1" }]
            }
        }),
    )
    .unwrap();
    // A tool devolve a request resultante.
    assert_eq!(upd["request"]["method"], "PUT");

    let c = carregar(&col);
    let rq = req_por_nome(&c, "Editavel").expect("Editavel");
    assert_eq!(rq.method, "PUT");
    assert_eq!(rq.url, "http://novo");
    assert_eq!(rq.headers.len(), 1);
    assert_eq!(rq.headers[0].name, "X-New");
    // O nome (nao tocado pelo patch) permanece.
    assert_eq!(rq.name, "Editavel");
}

#[test]
fn update_request_patch_vazio_e_noop() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Estavel", "method": "PATCH", "url": "http://x" }),
    )
    .unwrap();
    // patch = objeto vazio -> nada muda.
    call(
        "ruan_update_request",
        json!({ "collectionPath": col, "name": "Estavel", "patch": {} }),
    )
    .unwrap();
    let c = carregar(&col);
    let rq = req_por_nome(&c, "Estavel").unwrap();
    assert_eq!(rq.method, "PATCH");
    assert_eq!(rq.url, "http://x");
}

#[test]
fn update_request_patch_malformado_erra_sem_corromper() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Intacta", "method": "GET", "url": "http://ok" }),
    )
    .unwrap();
    // headers deve ser array de KeyValue; passar um numero quebra a desserializacao.
    let r = call(
        "ruan_update_request",
        json!({ "collectionPath": col, "name": "Intacta", "patch": { "headers": 42 } }),
    );
    assert!(r.is_err(), "patch malformado deveria errar");
    // O arquivo no disco continua valido e inalterado.
    let c = carregar(&col);
    let rq = req_por_nome(&c, "Intacta").expect("Intacta intacta");
    assert_eq!(rq.url, "http://ok");
    assert!(rq.headers.is_empty());
}

#[test]
fn update_request_rename_via_patch_move_o_arquivo() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Velho", "url": "http://v" }),
    )
    .unwrap();
    call(
        "ruan_update_request",
        json!({ "collectionPath": col, "name": "Velho", "patch": { "name": "Novinho" } }),
    )
    .unwrap();
    assert!(!Path::new(&col).join("velho.yml").exists());
    assert!(Path::new(&col).join("novinho.yml").is_file());
    let c = carregar(&col);
    assert!(req_por_nome(&c, "Velho").is_none());
    let rq = req_por_nome(&c, "Novinho").expect("Novinho");
    // O conteudo foi preservado no rename.
    assert_eq!(rq.url, "http://v");
}

#[test]
fn update_request_inexistente_erra() {
    let (_td, col) = col_temp();
    let r = call(
        "ruan_update_request",
        json!({ "collectionPath": col, "name": "NaoExiste", "patch": { "method": "GET" } }),
    );
    assert!(r.is_err());
}

#[test]
fn update_request_sem_patch_erra() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Tem" }),
    )
    .unwrap();
    // 'patch' e obrigatorio.
    let r = call(
        "ruan_update_request",
        json!({ "collectionPath": col, "name": "Tem" }),
    );
    assert!(r.is_err());
}

// ---------- rename / duplicate / move / delete ----------

#[test]
fn rename_item_request_e_folder() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "ReqA" }),
    )
    .unwrap();
    call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "PastaA" }),
    )
    .unwrap();

    // Renomeia a request.
    call(
        "ruan_rename_item",
        json!({ "collectionPath": col, "kind": "request", "oldName": "ReqA", "newName": "ReqB" }),
    )
    .unwrap();
    // Renomeia a pasta.
    call(
        "ruan_rename_item",
        json!({ "collectionPath": col, "kind": "folder", "oldName": "PastaA", "newName": "PastaB" }),
    )
    .unwrap();

    // Slug de "ReqA"/"ReqB" e "reqa"/"reqb" (token unico, sem hifen interno);
    // "PastaA"/"PastaB" viram "pastaa"/"pastab".
    assert!(!Path::new(&col).join("reqa.yml").exists());
    assert!(Path::new(&col).join("reqb.yml").is_file());
    assert!(!Path::new(&col).join("pastaa").exists());
    assert!(Path::new(&col).join("pastab").is_dir());
}

#[test]
fn rename_item_kind_invalido_erra() {
    let (_td, col) = col_temp();
    let r = call(
        "ruan_rename_item",
        json!({ "collectionPath": col, "kind": "xpto", "oldName": "a", "newName": "b" }),
    );
    assert!(r.is_err());
}

#[test]
fn duplicate_item_com_default_de_nome() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Base", "method": "DELETE", "url": "http://b" }),
    )
    .unwrap();
    // Sem newName -> default "<name> copia".
    let r = call(
        "ruan_duplicate_item",
        json!({ "collectionPath": col, "name": "Base", "seq": 2 }),
    )
    .unwrap();
    assert_eq!(
        PathBuf::from(r["path"].as_str().unwrap()).file_name().unwrap(),
        "base-copia.yml"
    );
    let c = carregar(&col);
    assert_eq!(c.items.len(), 2);
    let dup = req_por_nome(&c, "Base copia").expect("copia");
    assert_eq!(dup.method, "DELETE");
    assert_eq!(dup.url, "http://b");
    assert_eq!(dup.seq, 2);
}

#[test]
fn duplicate_item_com_new_name_explicito() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Src" }),
    )
    .unwrap();
    call(
        "ruan_duplicate_item",
        json!({ "collectionPath": col, "name": "Src", "newName": "Clone" }),
    )
    .unwrap();
    assert!(Path::new(&col).join("clone.yml").is_file());
}

#[test]
fn move_item_request_entre_pastas() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "Destino" }),
    )
    .unwrap();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Viajante" }),
    )
    .unwrap();
    // Move da raiz (fromDir ausente) para "destino" com novo seq.
    let r = call(
        "ruan_move_item",
        json!({
            "collectionPath": col,
            "kind": "request",
            "toDir": "destino",
            "name": "Viajante",
            "newSeq": 7
        }),
    )
    .unwrap();
    assert!(PathBuf::from(r["path"].as_str().unwrap()).starts_with(Path::new(&col).join("destino")));
    assert!(!Path::new(&col).join("viajante.yml").exists());

    let c = carregar(&col);
    let dst = c
        .items
        .iter()
        .find_map(|i| match i {
            TreeItem::Folder(f) if f.name == "Destino" => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(dst.items.len(), 1);
    match &dst.items[0] {
        TreeItem::Request(rq) => {
            assert_eq!(rq.name, "Viajante");
            assert_eq!(rq.seq, 7);
        }
        _ => panic!("esperava request movida"),
    }
}

#[test]
fn move_item_folder_entre_pastas() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "Alvo" }),
    )
    .unwrap();
    call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "Movel" }),
    )
    .unwrap();
    // request filha dentro de Movel para confirmar que vai junto.
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "dir": "movel", "name": "Filha" }),
    )
    .unwrap();
    call(
        "ruan_move_item",
        json!({
            "collectionPath": col,
            "kind": "folder",
            "toDir": "alvo",
            "name": "Movel",
            "newSeq": 0
        }),
    )
    .unwrap();
    assert!(!Path::new(&col).join("movel").exists());
    assert!(Path::new(&col).join("alvo").join("movel").join("filha.yml").is_file());
}

#[test]
fn move_item_kind_invalido_erra() {
    let (_td, col) = col_temp();
    let r = call(
        "ruan_move_item",
        json!({ "collectionPath": col, "kind": "nope", "name": "x" }),
    );
    assert!(r.is_err());
}

#[test]
fn delete_request_remove_arquivo() {
    let (_td, col) = col_temp();
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Descartavel" }),
    )
    .unwrap();
    assert!(Path::new(&col).join("descartavel.yml").is_file());
    call(
        "ruan_delete_request",
        json!({ "collectionPath": col, "name": "Descartavel" }),
    )
    .unwrap();
    assert!(!Path::new(&col).join("descartavel.yml").exists());
    assert!(carregar(&col).items.is_empty());
}

// ---------- SEGURANCA ----------

#[test]
fn nome_malicioso_request_e_rejeitado_sem_escrever_fora() {
    let (td, col) = col_temp();
    for nome in &["../escapa", "..", "a/b", "/etc/passwd", "C:\\x", "a\0b"] {
        let r = call(
            "ruan_create_request",
            json!({ "collectionPath": col, "name": nome }),
        );
        assert!(r.is_err(), "deveria rejeitar nome {nome:?}");
    }
    // Nada vazou para o pai do tempdir.
    assert!(!td.path().join("escapa.yml").exists());
    assert!(!td.path().join("escapa").exists());
}

#[test]
fn nome_malicioso_em_outras_tools() {
    let (_td, col) = col_temp();
    // create_folder com nome traversal.
    assert!(call(
        "ruan_create_folder",
        json!({ "collectionPath": col, "name": "../fora" })
    )
    .is_err());
    // rename para nome traversal.
    call(
        "ruan_create_request",
        json!({ "collectionPath": col, "name": "Ok" }),
    )
    .unwrap();
    assert!(call(
        "ruan_rename_item",
        json!({ "collectionPath": col, "kind": "request", "oldName": "Ok", "newName": "../x" })
    )
    .is_err());
    // duplicate com newName traversal.
    assert!(call(
        "ruan_duplicate_item",
        json!({ "collectionPath": col, "name": "Ok", "newName": "../x" })
    )
    .is_err());
}

#[test]
fn dir_traversal_simples_rejeitado() {
    let (_td, col) = col_temp();
    // "../fora" comeca com ".." e e barrado por dentro_de (caso ja coberto, mantido).
    let r = call(
        "ruan_create_request",
        json!({ "collectionPath": col, "dir": "../fora", "name": "x" }),
    );
    assert!(r.is_err());
}

// BUG REAL CONFIRMADO (ver retorno do agente): este teste FALHA hoje porque
// `mcp::resolver` confia em `fs_store::dentro_de`, cuja checagem e lexica e nao
// colapsa `..` quando os segmentos anteriores NAO existem em disco. Com um
// segmento inexistente seguido de `..`, o `dir` escapa a colecao e a request e
// gravada FORA dela. Mantido como teste que documenta/regride a correcao:
// quando `resolver` validar componente-a-componente (slug_seguro por segmento),
// estas chamadas passarao a errar e o arquivo nunca sera escrito fora.
#[test]
fn dir_traversal_com_segmento_inexistente_nao_escapa() {
    let (td, col) = col_temp();
    let fora = td.path().join("PWNED.yml"); // irmao da colecao
    let _ = std::fs::remove_file(&fora);

    let r = call(
        "ruan_create_request",
        json!({
            "collectionPath": col,
            "dir": "naoexiste/../..",
            "name": "PWNED"
        }),
    );

    // Invariante de seguranca: NADA pode ser escrito fora do diretorio da colecao.
    let escapou = fora.exists();
    if escapou {
        let _ = std::fs::remove_file(&fora);
    }
    assert!(
        !escapou,
        "FALHA DE SEGURANCA: request gravada fora da colecao via dir traversal ({})",
        fora.display()
    );
    // A consequencia esperada apos a correcao e um erro de dispatch.
    assert!(r.is_err(), "dir com segmento inexistente + '..' deveria ser rejeitado");
}

#[test]
fn dir_traversal_profundo_nao_escapa() {
    let (td, col) = col_temp();
    let fora = td.path().join("DEEP_PWNED.yml");
    let _ = std::fs::remove_file(&fora);

    let r = call(
        "ruan_create_request",
        json!({
            "collectionPath": col,
            "dir": "a/b/c/../../../..",
            "name": "DEEP_PWNED"
        }),
    );
    let escapou = fora.exists();
    if escapou {
        let _ = std::fs::remove_file(&fora);
    }
    assert!(
        !escapou,
        "FALHA DE SEGURANCA: escape via dir traversal profundo ({})",
        fora.display()
    );
    assert!(r.is_err());
}

// ---------- protocolo / dispatch ----------

#[test]
fn tool_desconhecida_vira_err_nao_panic() {
    let r = call("ruan_inexistente", json!({}));
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("desconhecida"));
}

#[test]
fn args_invalidos_viram_err_tratado() {
    // open_collection sem 'path'.
    assert!(call("ruan_open_collection", json!({})).is_err());
    // create_request sem collectionPath.
    assert!(call("ruan_create_request", json!({ "name": "x" })).is_err());
    // campo do tipo errado (path como numero, nao string).
    assert!(call("ruan_open_collection", json!({ "path": 123 })).is_err());
    // open_collection apontando para diretorio sem collection.yml.
    let td = TempDir::new().unwrap();
    assert!(call(
        "ruan_open_collection",
        json!({ "path": td.path().display().to_string() })
    )
    .is_err());
}

#[test]
fn lista_tools_expoe_nove_tools_com_prefixo_ruan() {
    let tools = lista_tools();
    let arr = tools.as_array().expect("array");
    assert_eq!(arr.len(), 9);
    let mut nomes: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
    nomes.sort();
    assert_eq!(
        nomes,
        vec![
            "ruan_create_collection",
            "ruan_create_folder",
            "ruan_create_request",
            "ruan_delete_request",
            "ruan_duplicate_item",
            "ruan_move_item",
            "ruan_open_collection",
            "ruan_rename_item",
            "ruan_update_request",
        ]
    );
    for t in arr {
        assert!(t["name"].as_str().unwrap().starts_with("ruan_"));
        assert!(t["inputSchema"].is_object(), "tool sem inputSchema");
        assert!(t["description"].is_string(), "tool sem description");
    }
}
