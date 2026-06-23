// Camada de I/O do store: le/grava a arvore da colecao no filesystem.
// Nomes de arquivo SEMPRE passam por `slug_seguro` antes de tocar o disco,
// garantindo que nada escape do diretorio da colecao.
//
// Layout em disco (file-based, YAML):
//   minha-colecao/
//     collection.yml          <- CollectionMeta
//     listar-usuarios.yml     <- RequestItem
//     auth/                    <- subpasta
//       folder.yml             <- FolderMeta
//       login.yml              <- RequestItem
//
// A arvore (`items`) e SEMPRE reconstruida a partir do disco; nunca confiamos
// num campo `items` serializado.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::store::error::StoreError;
use crate::store::models::{
    Collection, CollectionMeta, Folder, FolderMeta, RequestItem, TreeItem,
};
use crate::store::parser;
use crate::store::slug::slug_seguro;

const COLLECTION_FILE: &str = "collection.yml";
const FOLDER_FILE: &str = "folder.yml";
const YML_EXT: &str = "yml";

/// Limite de tamanho ao ler qualquer `.yml` de fonte nao-confiavel.
/// Defesa contra DoS: uma colecao maliciosa pode ter um arquivo gigante
/// (especialmente o `vars: serde_yaml::Value` nao-tipado, que aloca livremente).
/// 10 MB e folgado para configs reais de request/colecao.
pub const MAX_YAML_BYTES: u64 = 10 * 1024 * 1024;

/// Le um arquivo `.yml` aplicando o limite `MAX_YAML_BYTES`. Usa `take` para
/// nunca alocar mais que o limite + 1 byte; se o arquivo exceder, rejeita com
/// `ArquivoMuitoGrande` ANTES de parsear (o parser nunca ve dados ilimitados).
fn ler_yaml_limitado(path: &Path) -> Result<String, StoreError> {
    let file = File::open(path)?;
    // Le ate o limite + 1 para detectar overflow sem alocar o arquivo inteiro.
    let mut buf = Vec::new();
    let lido = file
        .take(MAX_YAML_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(StoreError::from)?;
    if lido as u64 > MAX_YAML_BYTES {
        return Err(StoreError::ArquivoMuitoGrande(path.display().to_string()));
    }
    String::from_utf8(buf).map_err(|e| StoreError::Io(e.to_string()))
}

/// Garante que `candidato` esta dentro de `base` apos canonicalizar.
/// Defesa em profundidade: mesmo que um slug passe, confirmamos que o caminho
/// final nao escapou do diretorio da colecao via symlink ou `..`.
///
/// RISCO CONHECIDO (TOCTOU, aceito para o escopo local single-user): existe uma
/// janela entre esta checagem (que canonicaliza, resolvendo symlinks) e a
/// escrita posterior (`fs::write`/`create_dir_all`/`remove_file`). Um atacante
/// com acesso de escrita ao diretorio da colecao poderia trocar um componente
/// por symlink nesse intervalo. Para este app local de usuario unico o impacto
/// e baixo e nao reescrevemos com O_NOFOLLOW/openat agora — revisitar se o
/// store passar a operar sobre diretorios compartilhados/multiusuario.
pub fn dentro_de(base: &Path, candidato: &Path) -> Result<(), StoreError> {
    // Canonicaliza a base (deve existir).
    let base_canon = base
        .canonicalize()
        .map_err(|_| StoreError::EscapaColecao(candidato.display().to_string()))?;
    // O candidato pode ainda nao existir; canonicaliza o ancestral existente
    // mais proximo e checa o prefixo.
    let mut ancestral = candidato;
    let alvo_canon = loop {
        if let Ok(c) = ancestral.canonicalize() {
            // Reanexa o sufixo nao-existente.
            let suffix = candidato
                .strip_prefix(ancestral)
                .unwrap_or_else(|_| Path::new(""));
            break c.join(suffix);
        }
        match ancestral.parent() {
            Some(p) => ancestral = p,
            None => break candidato.to_path_buf(),
        }
    };
    if alvo_canon.starts_with(&base_canon) {
        Ok(())
    } else {
        Err(StoreError::EscapaColecao(candidato.display().to_string()))
    }
}

/// Carrega uma colecao a partir do diretorio raiz, lendo a arvore recursivamente.
pub fn load_collection(dir: &Path) -> Result<Collection, StoreError> {
    let col_path = dir.join(COLLECTION_FILE);
    if !col_path.is_file() {
        return Err(StoreError::ColecaoNaoEncontrada(dir.display().to_string()));
    }
    let yaml = ler_yaml_limitado(&col_path)?;
    let meta: CollectionMeta = parser::parse_collection_meta(&yaml)?;

    let items = load_items(dir)?;

    Ok(Collection {
        name: meta.name,
        version: meta.version,
        items,
        vars: meta.vars,
    })
}

/// Le os filhos de um diretorio (requests .yml e subpastas), ordenados por `seq`
/// e depois por nome. Ignora `collection.yml`/`folder.yml` (sao metadados).
fn load_items(dir: &Path) -> Result<Vec<TreeItem>, StoreError> {
    let mut items: Vec<TreeItem> = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            // Subpasta: precisa ter folder.yml para ser considerada.
            let folder_meta_path = path.join(FOLDER_FILE);
            if folder_meta_path.is_file() {
                let folder = load_folder(&path)?;
                items.push(TreeItem::Folder(folder));
            }
        } else if file_type.is_file() {
            let nome = entry.file_name();
            let nome = nome.to_string_lossy();
            if nome == COLLECTION_FILE || nome == FOLDER_FILE {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some(YML_EXT) {
                let yaml = ler_yaml_limitado(&path)?;
                let req: RequestItem = parser::parse_request(&yaml)?;
                items.push(TreeItem::Request(req));
            }
        }
    }

    ordenar_items(&mut items);
    Ok(items)
}

/// Carrega uma pasta (folder.yml + filhos recursivos).
fn load_folder(dir: &Path) -> Result<Folder, StoreError> {
    let yaml = ler_yaml_limitado(&dir.join(FOLDER_FILE))?;
    let meta: FolderMeta = parser::parse_folder_meta(&yaml)?;
    let items = load_items(dir)?;
    Ok(Folder {
        name: meta.name,
        seq: meta.seq,
        items,
    })
}

/// Ordena irmaos por `seq` e desempata por nome (estavel e deterministico).
fn ordenar_items(items: &mut [TreeItem]) {
    items.sort_by(|a, b| a.seq().cmp(&b.seq()).then_with(|| a.name().cmp(b.name())));
}

/// Cria/inicializa o diretorio de uma colecao gravando o `collection.yml`.
pub fn save_collection_meta(dir: &Path, meta: &CollectionMeta) -> Result<(), StoreError> {
    fs::create_dir_all(dir)?;
    let yaml = parser::stringify_collection_meta(meta)?;
    fs::write(dir.join(COLLECTION_FILE), yaml)?;
    Ok(())
}

/// Grava uma request em `<dir>/<slug(name)>.yml`. `dir` deve estar dentro da
/// colecao. O nome da request e sanitizado antes de virar nome de arquivo.
pub fn save_request(
    collection_dir: &Path,
    dir: &Path,
    req: &RequestItem,
) -> Result<PathBuf, StoreError> {
    let slug = slug_seguro(&req.name)?;
    let file_name = format!("{slug}.{YML_EXT}");
    let alvo = dir.join(&file_name);
    dentro_de(collection_dir, &alvo)?;

    // TOCTOU conhecido/aceito: janela entre `dentro_de` e as escritas abaixo
    // (ver doc de `dentro_de`). Escopo local single-user.
    if let Some(parent) = alvo.parent() {
        fs::create_dir_all(parent)?;
    }
    let yaml = parser::stringify_request(req)?;
    fs::write(&alvo, yaml)?;
    Ok(alvo)
}

/// Cria uma subpasta `<dir>/<slug(name)>/` com seu `folder.yml`.
/// `dir` deve estar dentro da colecao; `name` e sanitizado.
pub fn create_folder(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
    seq: u32,
) -> Result<PathBuf, StoreError> {
    let slug = slug_seguro(name)?;
    let alvo = dir.join(&slug);
    dentro_de(collection_dir, &alvo)?;

    // TOCTOU conhecido/aceito: janela entre `dentro_de` e as escritas abaixo
    // (ver doc de `dentro_de`). Escopo local single-user.
    fs::create_dir_all(&alvo)?;
    let meta = FolderMeta {
        name: name.to_string(),
        seq,
    };
    let yaml = parser::stringify_folder_meta(&meta)?;
    fs::write(alvo.join(FOLDER_FILE), yaml)?;
    Ok(alvo)
}

/// Remove uma request pelo nome (sanitizado) dentro de `dir`.
pub fn delete_request(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
) -> Result<(), StoreError> {
    let slug = slug_seguro(name)?;
    let file_name = format!("{slug}.{YML_EXT}");
    let alvo = dir.join(&file_name);
    dentro_de(collection_dir, &alvo)?;
    // TOCTOU conhecido/aceito: janela entre `dentro_de` e o remove abaixo
    // (ver doc de `dentro_de`). Escopo local single-user.
    if alvo.is_file() {
        fs::remove_file(&alvo)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::models::{Auth, Body, Scripts};
    use std::io::Write;
    use tempfile::TempDir;

    fn req(name: &str, seq: u32) -> RequestItem {
        RequestItem {
            name: name.to_string(),
            seq,
            method: "GET".to_string(),
            url: String::new(),
            headers: vec![],
            params: vec![],
            body: Body::default(),
            auth: Auth::default(),
            scripts: Scripts::default(),
            tests: String::new(),
            docs: String::new(),
        }
    }

    /// Cria uma colecao basica num tempdir e devolve (tempdir, dir_da_colecao).
    fn col_temp() -> (TempDir, PathBuf) {
        let td = TempDir::new().unwrap();
        let dir = td.path().join("minha-colecao");
        let meta = CollectionMeta {
            name: "Minha Colecao".to_string(),
            version: "1".to_string(),
            vars: None,
        };
        save_collection_meta(&dir, &meta).unwrap();
        (td, dir)
    }

    // ---- load_collection / load_items ----

    #[test]
    fn load_collection_arvore_completa_e_ordenada() {
        let (_td, dir) = col_temp();

        // Requests na raiz, fora de ordem de seq.
        save_request(&dir, &dir, &req("Zebra", 1)).unwrap();
        save_request(&dir, &dir, &req("Alpha", 0)).unwrap();

        // Subpasta com folder.yml + uma request.
        let sub = create_folder(&dir, &dir, "auth", 5).unwrap();
        save_request(&dir, &sub, &req("login", 0)).unwrap();

        let col = load_collection(&dir).unwrap();
        assert_eq!(col.name, "Minha Colecao");
        assert_eq!(col.version, "1");
        // 2 requests + 1 pasta = 3 itens na raiz.
        assert_eq!(col.items.len(), 3);

        // Ordenacao por seq depois nome: Alpha(0), Zebra(1), auth(5).
        assert_eq!(col.items[0].name(), "Alpha");
        assert_eq!(col.items[1].name(), "Zebra");
        assert_eq!(col.items[2].name(), "auth");

        // A pasta carregou seu filho.
        match &col.items[2] {
            TreeItem::Folder(f) => {
                assert_eq!(f.seq, 5);
                assert_eq!(f.items.len(), 1);
                assert_eq!(f.items[0].name(), "login");
            }
            _ => panic!("esperava pasta"),
        }
    }

    #[test]
    fn load_items_desempata_por_nome_no_mesmo_seq() {
        let (_td, dir) = col_temp();
        save_request(&dir, &dir, &req("banana", 0)).unwrap();
        save_request(&dir, &dir, &req("abacate", 0)).unwrap();
        save_request(&dir, &dir, &req("cereja", 0)).unwrap();
        let col = load_collection(&dir).unwrap();
        let nomes: Vec<&str> = col.items.iter().map(|i| i.name()).collect();
        assert_eq!(nomes, vec!["abacate", "banana", "cereja"]);
    }

    #[test]
    fn load_collection_sem_arquivo_erra() {
        let td = TempDir::new().unwrap();
        let vazio = td.path().join("nao-existe");
        assert!(matches!(
            load_collection(&vazio),
            Err(StoreError::ColecaoNaoEncontrada(_))
        ));
    }

    #[test]
    fn load_items_ignora_diretorio_sem_folder_yml() {
        let (_td, dir) = col_temp();
        // Diretorio solto sem folder.yml NAO deve virar item.
        fs::create_dir_all(dir.join("solto")).unwrap();
        save_request(&dir, &dir, &req("unica", 0)).unwrap();
        let col = load_collection(&dir).unwrap();
        assert_eq!(col.items.len(), 1);
        assert_eq!(col.items[0].name(), "unica");
    }

    #[test]
    fn load_items_ignora_arquivos_nao_yml() {
        let (_td, dir) = col_temp();
        save_request(&dir, &dir, &req("real", 0)).unwrap();
        fs::write(dir.join("notas.txt"), "ignorar").unwrap();
        fs::write(dir.join("data.json"), "{}").unwrap();
        let col = load_collection(&dir).unwrap();
        assert_eq!(col.items.len(), 1);
        assert_eq!(col.items[0].name(), "real");
    }

    // ---- save_request round-trip de disco ----

    #[test]
    fn save_request_grava_yml_correto_e_rele_igual() {
        let (_td, dir) = col_temp();
        let mut r = req("Listar Usuarios", 2);
        r.method = "POST".to_string();
        r.url = "https://x/y".to_string();

        let alvo = save_request(&dir, &dir, &r).unwrap();
        // Nome de arquivo deve ser o slug.
        assert_eq!(alvo.file_name().unwrap(), "listar-usuarios.yml");
        assert!(alvo.is_file());

        // Reler pela colecao devolve a request igual.
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Request(lido) => assert_eq!(lido, &r),
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn save_request_em_subpasta_funciona() {
        let (_td, dir) = col_temp();
        let sub = create_folder(&dir, &dir, "auth", 0).unwrap();
        let alvo = save_request(&dir, &sub, &req("login", 0)).unwrap();
        assert!(alvo.starts_with(&sub));
        assert!(alvo.is_file());
    }

    // ---- create_folder ----

    #[test]
    fn create_folder_grava_folder_yml() {
        let (_td, dir) = col_temp();
        let alvo = create_folder(&dir, &dir, "Minha Pasta", 4).unwrap();
        assert_eq!(alvo.file_name().unwrap(), "minha-pasta"); // slug
        assert!(alvo.join(FOLDER_FILE).is_file());

        // O folder.yml preserva o nome original (nao o slug) e o seq.
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Folder(f) => {
                assert_eq!(f.name, "Minha Pasta");
                assert_eq!(f.seq, 4);
            }
            _ => panic!("esperava pasta"),
        }
    }

    // ---- delete_request ----

    #[test]
    fn delete_request_remove_arquivo() {
        let (_td, dir) = col_temp();
        let alvo = save_request(&dir, &dir, &req("temp", 0)).unwrap();
        assert!(alvo.is_file());
        delete_request(&dir, &dir, "temp").unwrap();
        assert!(!alvo.exists());
        let col = load_collection(&dir).unwrap();
        assert!(col.items.is_empty());
    }

    #[test]
    fn delete_request_inexistente_e_idempotente() {
        let (_td, dir) = col_temp();
        // Nome valido mas arquivo nao existe -> Ok sem erro.
        assert!(delete_request(&dir, &dir, "nao-existe").is_ok());
    }

    // ---- SEGURANCA: nomes maliciosos sao rejeitados e nada vaza ----

    #[test]
    fn save_request_nome_malicioso_e_rejeitado() {
        let (_td, dir) = col_temp();
        for nome in &["..", "a/b", "/etc/passwd", "..\\x", "C:\\x", "a\0b"] {
            let r = req(nome, 0);
            let res = save_request(&dir, &dir, &r);
            assert!(
                res.is_err(),
                "save_request deveria rejeitar nome {nome:?}"
            );
        }
    }

    #[test]
    fn create_folder_nome_malicioso_e_rejeitado() {
        let (_td, dir) = col_temp();
        for nome in &["..", "../escapa", "a/b", "/abs", "C:\\x"] {
            let res = create_folder(&dir, &dir, nome, 0);
            assert!(res.is_err(), "create_folder deveria rejeitar {nome:?}");
        }
    }

    #[test]
    fn delete_request_nome_malicioso_e_rejeitado() {
        let (_td, dir) = col_temp();
        for nome in &["..", "a/b", "/etc/passwd"] {
            let res = delete_request(&dir, &dir, nome);
            assert!(res.is_err(), "delete_request deveria rejeitar {nome:?}");
        }
    }

    #[test]
    fn save_request_dir_fora_da_colecao_e_rejeitado() {
        let (_td, dir) = col_temp();
        // dir aponta para o PAI da colecao: o slug e valido, mas `dentro_de`
        // deve barrar porque o alvo final cai fora da base.
        let fora = dir.parent().unwrap().to_path_buf();
        let res = save_request(&dir, &fora, &req("vaza", 0));
        assert!(matches!(res, Err(StoreError::EscapaColecao(_))));
        // E nada foi escrito fora da colecao.
        assert!(!fora.join("vaza.yml").exists());
    }

    #[test]
    fn create_folder_dir_fora_da_colecao_e_rejeitado() {
        let (_td, dir) = col_temp();
        let fora = dir.parent().unwrap().to_path_buf();
        let res = create_folder(&dir, &fora, "vaza", 0);
        assert!(matches!(res, Err(StoreError::EscapaColecao(_))));
        assert!(!fora.join("vaza").exists());
    }

    #[test]
    fn nada_e_escrito_fora_da_base_em_nome_malicioso() {
        let (_td, dir) = col_temp();
        let antes: Vec<_> = fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        let _ = save_request(&dir, &dir, &req("../../escapou", 0));
        let _ = create_folder(&dir, &dir, "../../pasta", 0);
        let depois: Vec<_> = fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        // O conteudo do diretorio pai nao mudou.
        assert_eq!(antes.len(), depois.len());
    }

    // ---- dentro_de ----

    #[test]
    fn dentro_de_aceita_caminho_interno() {
        let (_td, dir) = col_temp();
        let alvo = dir.join("sub").join("x.yml");
        assert!(dentro_de(&dir, &alvo).is_ok());
    }

    #[test]
    fn dentro_de_rejeita_caminho_externo() {
        let (_td, dir) = col_temp();
        let fora = dir.parent().unwrap().join("outro.yml");
        assert!(matches!(
            dentro_de(&dir, &fora),
            Err(StoreError::EscapaColecao(_))
        ));
    }

    #[test]
    fn dentro_de_rejeita_traversal_via_dotdot() {
        let (_td, dir) = col_temp();
        let escapa = dir.join("..").join("escapou.yml");
        assert!(matches!(
            dentro_de(&dir, &escapa),
            Err(StoreError::EscapaColecao(_))
        ));
    }

    #[test]
    fn dentro_de_base_inexistente_erra() {
        let td = TempDir::new().unwrap();
        let base = td.path().join("nao-existe");
        let alvo = base.join("x.yml");
        assert!(matches!(
            dentro_de(&base, &alvo),
            Err(StoreError::EscapaColecao(_))
        ));
    }

    // ---- MAX_YAML_BYTES ----

    #[test]
    fn max_yaml_bytes_e_10_mib() {
        // Trava o valor concreto da constante (10 MiB). Sem isto, mutantes que
        // trocam `*` por `+` em `10 * 1024 * 1024` passam despercebidos, pois os
        // testes de limite usam a propria constante como referencia.
        assert_eq!(MAX_YAML_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_YAML_BYTES, 10_485_760);
    }

    #[test]
    fn ler_yaml_dentro_do_limite_funciona() {
        let (_td, dir) = col_temp();
        // Cria uma request grande mas dentro do limite.
        let mut r = req("grande", 0);
        r.docs = "x".repeat(1024); // 1KB, bem abaixo de 10MB
        save_request(&dir, &dir, &r).unwrap();
        let col = load_collection(&dir).unwrap();
        assert_eq!(col.items.len(), 1);
    }

    #[test]
    fn ler_yaml_acima_do_limite_e_rejeitado() {
        let (_td, dir) = col_temp();
        // Escreve um .yml gigante diretamente (> MAX_YAML_BYTES).
        let path = dir.join("gigante.yml");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"name: grande\ndocs: \"").unwrap();
        // Escreve > 10MB de conteudo.
        let chunk = vec![b'a'; 1024 * 1024];
        for _ in 0..11 {
            f.write_all(&chunk).unwrap();
        }
        f.write_all(b"\"\n").unwrap();
        f.flush().unwrap();
        drop(f);

        // Ler diretamente deve dar ArquivoMuitoGrande.
        assert!(matches!(
            ler_yaml_limitado(&path),
            Err(StoreError::ArquivoMuitoGrande(_))
        ));
        // E carregar a colecao tambem propaga o erro (nao parseia o gigante).
        assert!(matches!(
            load_collection(&dir),
            Err(StoreError::ArquivoMuitoGrande(_))
        ));
    }

    #[test]
    fn ler_yaml_exatamente_no_limite_e_aceito() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("limite.yml");
        // Exatamente MAX_YAML_BYTES bytes -> aceito (limite e inclusivo).
        let data = vec![b'a'; MAX_YAML_BYTES as usize];
        fs::write(&path, &data).unwrap();
        // Conteudo nao e YAML valido de RequestItem, mas a LEITURA deve passar.
        assert!(ler_yaml_limitado(&path).is_ok());
    }

    #[test]
    fn ler_yaml_um_byte_acima_do_limite_e_rejeitado() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("acima.yml");
        let data = vec![b'a'; (MAX_YAML_BYTES + 1) as usize];
        fs::write(&path, &data).unwrap();
        assert!(matches!(
            ler_yaml_limitado(&path),
            Err(StoreError::ArquivoMuitoGrande(_))
        ));
    }
}
