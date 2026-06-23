// Operacoes de arvore (CRUD + mover/reordenar) da feature F3.
//
// Este modulo COMPLEMENTA `fs_store` com as operacoes que faltavam para a
// sidebar: criar request, renomear, duplicar e mover/reordenar. Tudo aqui
// reusa as primitivas PUBLICAS e seguras de `fs_store` (`save_request`,
// `create_folder`, `delete_request`, `dentro_de`) e a sanitizacao de
// `slug::slug_seguro`.
//
// SEGURANCA: todo nome de request/pasta vindo do front e NAO-CONFIAVEL. Antes
// de qualquer toque no disco, validamos com `slug_seguro` (rejeita traversal e
// separadores) e confirmamos com `dentro_de` que origem E destino caem dentro
// da colecao. Os comandos `pub fn` deste modulo sao registrados no lib.rs pela
// fase de Integracao.
//
// TOCTOU conhecido/aceito (escopo local single-user): mesmas janelas
// documentadas em `fs_store::dentro_de`. Nao reescrevemos com openat/O_NOFOLLOW.

use std::fs;
use std::path::{Path, PathBuf};

use crate::store::error::StoreError;
use crate::store::fs_store;
use crate::store::models::{FolderMeta, RequestItem};
use crate::store::parser;
use crate::store::slug::slug_seguro;

const FOLDER_FILE: &str = "folder.yml";
const YML_EXT: &str = "yml";

/// Monta o caminho de arquivo de uma request a partir do nome (sanitizado),
/// validando que cai dentro da colecao. Nao toca o disco.
fn caminho_request(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
) -> Result<PathBuf, StoreError> {
    let slug = slug_seguro(name)?;
    let alvo = dir.join(format!("{slug}.{YML_EXT}"));
    fs_store::dentro_de(collection_dir, &alvo)?;
    Ok(alvo)
}

/// Monta o caminho de diretorio de uma pasta a partir do nome (sanitizado),
/// validando que cai dentro da colecao. Nao toca o disco.
fn caminho_folder(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
) -> Result<PathBuf, StoreError> {
    let slug = slug_seguro(name)?;
    let alvo = dir.join(slug);
    fs_store::dentro_de(collection_dir, &alvo)?;
    Ok(alvo)
}

/// Cria uma request nova (GET vazia) chamada `name` dentro de `dir`.
/// `seq` define a ordem de exibicao. Reusa `fs_store::save_request`, que
/// sanitiza o nome e valida o caminho. Devolve o caminho do arquivo gravado.
pub fn create_request(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
    seq: u32,
) -> Result<PathBuf, StoreError> {
    let req = request_default(name, seq);
    fs_store::save_request(collection_dir, dir, &req)
}

/// Constroi uma `RequestItem` padrao (GET, sem url/headers/params/body/auth).
/// Espelha `novaRequest` do front (`src/lib/types.ts`).
pub fn request_default(name: &str, seq: u32) -> RequestItem {
    RequestItem {
        name: name.to_string(),
        seq,
        method: "GET".to_string(),
        url: String::new(),
        headers: Vec::new(),
        params: Vec::new(),
        body: Default::default(),
        auth: Default::default(),
        scripts: Default::default(),
        tests: String::new(),
        docs: String::new(),
    }
}

/// Renomeia uma request dentro de `dir`: move `<slug(old)>.yml` para
/// `<slug(new)>.yml` e reescreve o campo `name` no conteudo.
///
/// Idempotente quando old==new (slug igual). Se o destino ja existir com um
/// slug diferente, sobrescreve (mesma semantica de `fs::rename`). Valida ambos
/// os caminhos com `dentro_de`.
pub fn rename_request(
    collection_dir: &Path,
    dir: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<PathBuf, StoreError> {
    let origem = caminho_request(collection_dir, dir, old_name)?;
    if !origem.is_file() {
        return Err(StoreError::Io(format!(
            "request inexistente: {}",
            origem.display()
        )));
    }
    // Le, atualiza o nome e regrava sob o novo slug.
    let yaml = fs::read_to_string(&origem)?;
    let mut req = parser::parse_request(&yaml)?;
    req.name = new_name.to_string();
    let destino = fs_store::save_request(collection_dir, dir, &req)?;
    // Se o slug mudou, remove o arquivo antigo. (Mesmo slug => destino==origem.)
    if destino != origem {
        fs::remove_file(&origem)?;
    }
    Ok(destino)
}

/// Renomeia uma pasta dentro de `dir`: move o diretorio `<slug(old)>/` para
/// `<slug(new)>/` e reescreve o `name` no `folder.yml`, preservando os filhos.
pub fn rename_folder(
    collection_dir: &Path,
    dir: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<PathBuf, StoreError> {
    let origem = caminho_folder(collection_dir, dir, old_name)?;
    if !origem.is_dir() {
        return Err(StoreError::Io(format!(
            "pasta inexistente: {}",
            origem.display()
        )));
    }
    let destino = caminho_folder(collection_dir, dir, new_name)?;
    // Le o seq atual do folder.yml para preservar a ordem.
    let seq = ler_seq_folder(&origem)?;
    if destino != origem {
        if destino.exists() {
            return Err(StoreError::Io(format!(
                "destino ja existe: {}",
                destino.display()
            )));
        }
        fs::rename(&origem, &destino)?;
    }
    // Reescreve o folder.yml com o novo nome (mesmo quando o slug nao mudou).
    escrever_folder_meta(&destino, new_name, seq)?;
    Ok(destino)
}

/// Duplica uma request dentro de `dir`: le `<slug(name)>.yml` e grava uma copia
/// com `new_name` (default "<name> copia" calculado no front). Devolve o caminho
/// da copia. O `seq` da copia e o informado (a sidebar reordena depois).
pub fn duplicate_request(
    collection_dir: &Path,
    dir: &Path,
    name: &str,
    new_name: &str,
    seq: u32,
) -> Result<PathBuf, StoreError> {
    let origem = caminho_request(collection_dir, dir, name)?;
    if !origem.is_file() {
        return Err(StoreError::Io(format!(
            "request inexistente: {}",
            origem.display()
        )));
    }
    let yaml = fs::read_to_string(&origem)?;
    let mut req = parser::parse_request(&yaml)?;
    req.name = new_name.to_string();
    req.seq = seq;
    fs_store::save_request(collection_dir, dir, &req)
}

/// Move uma request de `from_dir` para `to_dir`, opcionalmente reatribuindo o
/// `seq` (reordenacao). Ambos os diretorios devem estar dentro da colecao.
/// Quando `from_dir == to_dir`, vira apenas uma atualizacao de `seq` no lugar.
pub fn move_request(
    collection_dir: &Path,
    from_dir: &Path,
    to_dir: &Path,
    name: &str,
    new_seq: u32,
) -> Result<PathBuf, StoreError> {
    let origem = caminho_request(collection_dir, from_dir, name)?;
    if !origem.is_file() {
        return Err(StoreError::Io(format!(
            "request inexistente: {}",
            origem.display()
        )));
    }
    let yaml = fs::read_to_string(&origem)?;
    let mut req = parser::parse_request(&yaml)?;
    req.seq = new_seq;
    // Grava no destino (valida `to_dir` via dentro_de dentro do save_request).
    let destino = fs_store::save_request(collection_dir, to_dir, &req)?;
    if destino != origem {
        fs::remove_file(&origem)?;
    }
    Ok(destino)
}

/// Move uma pasta de `from_dir` para `to_dir`, reatribuindo o `seq`. Move o
/// diretorio inteiro (com os filhos) e reescreve o `folder.yml` com o novo seq.
/// Rejeita mover uma pasta para dentro de si mesma (loop no filesystem).
pub fn move_folder(
    collection_dir: &Path,
    from_dir: &Path,
    to_dir: &Path,
    name: &str,
    new_seq: u32,
) -> Result<PathBuf, StoreError> {
    let origem = caminho_folder(collection_dir, from_dir, name)?;
    if !origem.is_dir() {
        return Err(StoreError::Io(format!(
            "pasta inexistente: {}",
            origem.display()
        )));
    }
    let destino = caminho_folder(collection_dir, to_dir, name)?;
    if destino != origem {
        // Impede mover a pasta para dentro de si mesma (ou de um descendente).
        if destino.starts_with(&origem) {
            return Err(StoreError::Io(format!(
                "nao e possivel mover '{}' para dentro de si mesma",
                origem.display()
            )));
        }
        if destino.exists() {
            return Err(StoreError::Io(format!(
                "destino ja existe: {}",
                destino.display()
            )));
        }
        fs::rename(&origem, &destino)?;
    }
    // Atualiza o seq no folder.yml (preserva o nome original).
    let nome = ler_nome_folder(&destino)?;
    escrever_folder_meta(&destino, &nome, new_seq)?;
    Ok(destino)
}

/// Le o `seq` do `folder.yml` de `folder_dir`.
fn ler_seq_folder(folder_dir: &Path) -> Result<u32, StoreError> {
    let meta = ler_folder_meta(folder_dir)?;
    Ok(meta.seq)
}

/// Le o `name` do `folder.yml` de `folder_dir`.
fn ler_nome_folder(folder_dir: &Path) -> Result<String, StoreError> {
    let meta = ler_folder_meta(folder_dir)?;
    Ok(meta.name)
}

/// Le e parseia o `folder.yml` de `folder_dir`.
fn ler_folder_meta(folder_dir: &Path) -> Result<FolderMeta, StoreError> {
    let yaml = fs::read_to_string(folder_dir.join(FOLDER_FILE))?;
    parser::parse_folder_meta(&yaml)
}

/// Reescreve o `folder.yml` de `folder_dir` com `name`/`seq`.
fn escrever_folder_meta(
    folder_dir: &Path,
    name: &str,
    seq: u32,
) -> Result<(), StoreError> {
    let meta = FolderMeta {
        name: name.to_string(),
        seq,
        auth: None,
    };
    let yaml = parser::stringify_folder_meta(&meta)?;
    fs::write(folder_dir.join(FOLDER_FILE), yaml)?;
    Ok(())
}

// ---- Comandos IPC (registrar no lib.rs na Integracao) ----------------------

/// Resolve um `dir` opcional (subdiretorio relativo, input NAO-confiavel) em um
/// caminho absoluto dentro da colecao. None => raiz da colecao.
fn resolver_dir(
    collection_dir: &Path,
    dir: Option<String>,
) -> Result<PathBuf, StoreError> {
    let target = match dir {
        Some(d) => collection_dir.join(d),
        None => collection_dir.to_path_buf(),
    };
    fs_store::dentro_de(collection_dir, &target)?;
    Ok(target)
}

/// Comando: cria uma request nova (GET vazia) em `dir` (ou raiz).
#[tauri::command]
pub fn create_request_cmd(
    collection_path: String,
    dir: Option<String>,
    name: String,
    seq: u32,
) -> Result<String, StoreError> {
    let collection_dir = PathBuf::from(&collection_path);
    let target = resolver_dir(&collection_dir, dir)?;
    let written = create_request(&collection_dir, &target, &name, seq)?;
    Ok(written.display().to_string())
}

/// Comando: renomeia um item. `kind` e "folder" ou "request".
#[tauri::command]
pub fn rename_item(
    collection_path: String,
    dir: Option<String>,
    kind: String,
    old_name: String,
    new_name: String,
) -> Result<String, StoreError> {
    let collection_dir = PathBuf::from(&collection_path);
    let target = resolver_dir(&collection_dir, dir)?;
    let res = match kind.as_str() {
        "folder" => rename_folder(&collection_dir, &target, &old_name, &new_name)?,
        "request" => rename_request(&collection_dir, &target, &old_name, &new_name)?,
        outro => return Err(StoreError::InvalidName(format!("kind invalido: {outro}"))),
    };
    Ok(res.display().to_string())
}

/// Comando: duplica uma request em `dir`.
#[tauri::command]
pub fn duplicate_item(
    collection_path: String,
    dir: Option<String>,
    name: String,
    new_name: String,
    seq: u32,
) -> Result<String, StoreError> {
    let collection_dir = PathBuf::from(&collection_path);
    let target = resolver_dir(&collection_dir, dir)?;
    let res = duplicate_request(&collection_dir, &target, &name, &new_name, seq)?;
    Ok(res.display().to_string())
}

/// Comando: move/reordena um item. `kind` e "folder" ou "request".
/// `from_dir`/`to_dir` sao subdiretorios relativos a colecao (None => raiz).
#[tauri::command]
pub fn move_item(
    collection_path: String,
    kind: String,
    from_dir: Option<String>,
    to_dir: Option<String>,
    name: String,
    new_seq: u32,
) -> Result<String, StoreError> {
    let collection_dir = PathBuf::from(&collection_path);
    let from = resolver_dir(&collection_dir, from_dir)?;
    let to = resolver_dir(&collection_dir, to_dir)?;
    let res = match kind.as_str() {
        "folder" => move_folder(&collection_dir, &from, &to, &name, new_seq)?,
        "request" => move_request(&collection_dir, &from, &to, &name, new_seq)?,
        outro => return Err(StoreError::InvalidName(format!("kind invalido: {outro}"))),
    };
    Ok(res.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fs_store::{create_folder, load_collection, save_collection_meta};
    use crate::store::models::{CollectionMeta, TreeItem};
    use tempfile::TempDir;

    /// Cria uma colecao basica num tempdir e devolve (tempdir, dir_da_colecao).
    fn col_temp() -> (TempDir, PathBuf) {
        let td = TempDir::new().unwrap();
        let dir = td.path().join("minha-colecao");
        let meta = CollectionMeta {
            name: "Minha Colecao".to_string(),
            version: "1".to_string(),
            vars: None,
            auth: None,
        };
        save_collection_meta(&dir, &meta).unwrap();
        (td, dir)
    }

    // ---- create_request ----

    #[test]
    fn create_request_grava_get_vazia() {
        let (_td, dir) = col_temp();
        let alvo = create_request(&dir, &dir, "Nova Request", 3).unwrap();
        assert_eq!(alvo.file_name().unwrap(), "nova-request.yml");
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Request(r) => {
                assert_eq!(r.name, "Nova Request");
                assert_eq!(r.method, "GET");
                assert_eq!(r.seq, 3);
                assert_eq!(r.url, "");
            }
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn request_default_e_get_vazia() {
        let r = request_default("X", 7);
        assert_eq!(r.method, "GET");
        assert_eq!(r.seq, 7);
        assert!(r.headers.is_empty());
        assert!(r.params.is_empty());
        assert_eq!(r.url, "");
    }

    #[test]
    fn create_request_nome_malicioso_rejeitado() {
        let (_td, dir) = col_temp();
        assert!(create_request(&dir, &dir, "../escapa", 0).is_err());
    }

    // ---- rename_request ----

    #[test]
    fn rename_request_move_arquivo_e_atualiza_nome() {
        let (_td, dir) = col_temp();
        create_request(&dir, &dir, "Antigo", 0).unwrap();
        let novo = rename_request(&dir, &dir, "Antigo", "Novo Nome").unwrap();
        assert_eq!(novo.file_name().unwrap(), "novo-nome.yml");
        assert!(!dir.join("antigo.yml").exists());
        let col = load_collection(&dir).unwrap();
        assert_eq!(col.items.len(), 1);
        match &col.items[0] {
            TreeItem::Request(r) => assert_eq!(r.name, "Novo Nome"),
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn rename_request_mesmo_slug_so_atualiza_nome() {
        let (_td, dir) = col_temp();
        // "Hello" e "hello" tem o mesmo slug -> destino == origem, sem remover.
        create_request(&dir, &dir, "Hello", 0).unwrap();
        let res = rename_request(&dir, &dir, "Hello", "hello").unwrap();
        assert_eq!(res.file_name().unwrap(), "hello.yml");
        assert!(res.is_file());
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Request(r) => assert_eq!(r.name, "hello"),
            _ => panic!("esperava request"),
        }
    }

    #[test]
    fn rename_request_inexistente_erra() {
        let (_td, dir) = col_temp();
        assert!(rename_request(&dir, &dir, "nada", "outro").is_err());
    }

    // ---- rename_folder ----

    #[test]
    fn rename_folder_move_dir_e_preserva_filhos() {
        let (_td, dir) = col_temp();
        let sub = create_folder(&dir, &dir, "Auth", 2).unwrap();
        create_request(&dir, &sub, "login", 0).unwrap();
        let novo = rename_folder(&dir, &dir, "Auth", "Autenticacao").unwrap();
        assert_eq!(novo.file_name().unwrap(), "autenticacao");
        assert!(!dir.join("auth").exists());
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Folder(f) => {
                assert_eq!(f.name, "Autenticacao");
                assert_eq!(f.seq, 2); // seq preservado
                assert_eq!(f.items.len(), 1);
                assert_eq!(f.items[0].name(), "login");
            }
            _ => panic!("esperava pasta"),
        }
    }

    #[test]
    fn rename_folder_destino_existente_erra() {
        let (_td, dir) = col_temp();
        create_folder(&dir, &dir, "a", 0).unwrap();
        create_folder(&dir, &dir, "b", 1).unwrap();
        // Renomear "a" para "b" colide.
        assert!(rename_folder(&dir, &dir, "a", "b").is_err());
    }

    // ---- duplicate_request ----

    #[test]
    fn duplicate_request_copia_conteudo() {
        let (_td, dir) = col_temp();
        let mut r = request_default("Original", 0);
        r.method = "POST".to_string();
        r.url = "https://x".to_string();
        fs_store::save_request(&dir, &dir, &r).unwrap();
        let copia = duplicate_request(&dir, &dir, "Original", "Original copia", 1).unwrap();
        assert_eq!(copia.file_name().unwrap(), "original-copia.yml");
        let col = load_collection(&dir).unwrap();
        assert_eq!(col.items.len(), 2);
        // A copia preserva metodo/url e tem o novo nome/seq.
        let dup = col
            .items
            .iter()
            .find_map(|i| match i {
                TreeItem::Request(r) if r.name == "Original copia" => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(dup.method, "POST");
        assert_eq!(dup.url, "https://x");
        assert_eq!(dup.seq, 1);
    }

    // ---- move_request ----

    #[test]
    fn move_request_entre_pastas() {
        let (_td, dir) = col_temp();
        let sub = create_folder(&dir, &dir, "destino", 0).unwrap();
        create_request(&dir, &dir, "viajante", 0).unwrap();
        let novo = move_request(&dir, &dir, &sub, "viajante", 5).unwrap();
        assert!(novo.starts_with(&sub));
        assert!(!dir.join("viajante.yml").exists());
        let col = load_collection(&dir).unwrap();
        // Raiz agora tem so a pasta.
        match &col.items[0] {
            TreeItem::Folder(f) => {
                assert_eq!(f.items.len(), 1);
                match &f.items[0] {
                    TreeItem::Request(r) => assert_eq!(r.seq, 5),
                    _ => panic!("esperava request"),
                }
            }
            _ => panic!("esperava pasta"),
        }
    }

    #[test]
    fn move_request_mesmo_dir_so_muda_seq() {
        let (_td, dir) = col_temp();
        create_request(&dir, &dir, "fixa", 0).unwrap();
        let res = move_request(&dir, &dir, &dir, "fixa", 9).unwrap();
        assert!(res.is_file());
        let col = load_collection(&dir).unwrap();
        match &col.items[0] {
            TreeItem::Request(r) => assert_eq!(r.seq, 9),
            _ => panic!("esperava request"),
        }
    }

    // ---- move_folder ----

    #[test]
    fn move_folder_entre_pastas() {
        let (_td, dir) = col_temp();
        let destino = create_folder(&dir, &dir, "destino", 0).unwrap();
        let movel = create_folder(&dir, &dir, "movel", 1).unwrap();
        create_request(&dir, &movel, "filho", 0).unwrap();
        let novo = move_folder(&dir, &dir, &destino, "movel", 3).unwrap();
        assert!(novo.starts_with(&destino));
        assert!(!dir.join("movel").exists());
        let col = load_collection(&dir).unwrap();
        // "destino" contem "movel" que contem "filho".
        let dst = col
            .items
            .iter()
            .find_map(|i| match i {
                TreeItem::Folder(f) if f.name == "destino" => Some(f),
                _ => None,
            })
            .unwrap();
        match &dst.items[0] {
            TreeItem::Folder(f) => {
                assert_eq!(f.name, "movel");
                assert_eq!(f.seq, 3);
                assert_eq!(f.items[0].name(), "filho");
            }
            _ => panic!("esperava pasta aninhada"),
        }
    }

    #[test]
    fn move_folder_para_dentro_de_si_erra() {
        let (_td, dir) = col_temp();
        let pasta = create_folder(&dir, &dir, "pai", 0).unwrap();
        // Tentar mover "pai" para dentro de "pai" (destino comeca com origem).
        let res = move_folder(&dir, &dir, &pasta, "pai", 0);
        assert!(res.is_err());
        // O diretorio original continua intacto.
        assert!(dir.join("pai").is_dir());
    }

    // ---- seguranca: dir fora da colecao ----

    #[test]
    fn resolver_dir_fora_da_colecao_erra() {
        let (_td, dir) = col_temp();
        assert!(resolver_dir(&dir, Some("../fora".to_string())).is_err());
    }

    #[test]
    fn resolver_dir_none_e_raiz() {
        let (_td, dir) = col_temp();
        let r = resolver_dir(&dir, None).unwrap();
        assert_eq!(r, dir);
    }
}
