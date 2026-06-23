// F2 — Operacoes de CRUD no nivel de colecao (criar uma colecao nova).
//
// "Abrir" reusa `open_collection` (F1). Aqui implementamos so a CRIACAO: dado um
// diretorio-pai e um nome de colecao escolhidos pelo usuario, criamos a pasta da
// colecao (<parent>/<slug(name)>/) com seu `collection.yml` inicial.
//
// SEGURANCA: o `name` vem do usuario e NUNCA pode escapar do diretorio-pai. Por
// isso passa por `slug_seguro` (valida traversal/absoluto/NUL e devolve um unico
// componente [a-z0-9-]) ANTES de virar nome de pasta. Confirmamos tambem, via
// `dentro_de`, que o alvo final cai dentro de `parent` (defesa em profundidade).
//
// Nao sobrescrevemos: se ja existir uma colecao naquele caminho, erramos.

use std::path::{Path, PathBuf};

use crate::store::error::StoreError;
use crate::store::fs_store::{self, dentro_de};
use crate::store::models::{Collection, CollectionMeta};
use crate::store::slug::slug_seguro;

const COLLECTION_FILE: &str = "collection.yml";

/// Cria uma colecao nova em `<parent>/<slug(name)>/` com um `collection.yml`
/// inicial (versao "1", sem vars). Retorna a `Collection` carregada do disco
/// (arvore vazia), pronta pra entrar no estado do app.
///
/// Erros:
/// - `InvalidName` / `PathTraversal` se `name` for invalido/malicioso.
/// - `EscapaColecao` se o alvo cair fora de `parent` (nao deveria, com slug, mas
///   checamos por seguranca).
/// - `Io` ("ja existe") se ja houver um `collection.yml` no alvo — nao
///   sobrescrevemos. (Reusa a variante `Io` por nao podermos, nesta onda,
///   adicionar uma variante dedicada em `error.rs`.)
/// - `Io` em falha de filesystem.
pub fn create_collection(parent: &Path, name: &str) -> Result<Collection, StoreError> {
    // 1. Sanitiza o nome -> slug seguro de um unico componente.
    let slug = slug_seguro(name)?;

    // 2. Monta o alvo e confirma que esta dentro do diretorio-pai.
    let alvo = parent.join(&slug);
    dentro_de(parent, &alvo)?;

    // 3. Nao sobrescreve uma colecao existente.
    if alvo.join(COLLECTION_FILE).is_file() {
        return Err(StoreError::Io(format!(
            "ja existe uma colecao em: '{}'",
            alvo.display()
        )));
    }

    // 4. Grava o collection.yml inicial (preserva o NOME original, nao o slug).
    let meta = CollectionMeta {
        name: name.trim().to_string(),
        version: "1".to_string(),
        vars: None,
        auth: None,
    };
    fs_store::save_collection_meta(&alvo, &meta)?;

    // 5. Recarrega do disco pra devolver a arvore canonica (vazia).
    fs_store::load_collection(&alvo)
}

/// Variante de conveniencia que recebe `String`s (vindas do IPC) e devolve a
/// colecao junto do caminho final criado, util pro front saber onde indexar.
pub fn create_collection_at(parent: String, name: String) -> Result<(PathBuf, Collection), StoreError> {
    let parent_path = PathBuf::from(&parent);
    let slug = slug_seguro(&name)?;
    let alvo = parent_path.join(&slug);
    let col = create_collection(&parent_path, &name)?;
    Ok((alvo, col))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::models::TreeItem;
    use tempfile::TempDir;

    #[test]
    fn create_collection_cria_pasta_e_yml() {
        let td = TempDir::new().unwrap();
        let col = create_collection(td.path(), "Minha API").unwrap();
        assert_eq!(col.name, "Minha API");
        assert_eq!(col.version, "1");
        assert!(col.items.is_empty());
        // A pasta usa o slug; o collection.yml existe.
        let dir = td.path().join("minha-api");
        assert!(dir.join("collection.yml").is_file());
    }

    #[test]
    fn create_collection_preserva_nome_original() {
        let td = TempDir::new().unwrap();
        let col = create_collection(td.path(), "  Olá Mundo  ").unwrap();
        // Nome trim-ado preservado; pasta usa slug ascii.
        assert_eq!(col.name, "Olá Mundo");
        assert!(td.path().join("ola-mundo").join("collection.yml").is_file());
    }

    #[test]
    fn create_collection_arvore_vazia() {
        let td = TempDir::new().unwrap();
        let col = create_collection(td.path(), "vazia").unwrap();
        let n: Vec<&TreeItem> = col.items.iter().collect();
        assert!(n.is_empty());
    }

    #[test]
    fn create_collection_nao_sobrescreve_existente() {
        let td = TempDir::new().unwrap();
        create_collection(td.path(), "dup").unwrap();
        let res = create_collection(td.path(), "dup");
        assert!(matches!(res, Err(StoreError::Io(_))));
    }

    #[test]
    fn create_collection_nome_malicioso_e_rejeitado() {
        let td = TempDir::new().unwrap();
        for nome in &["..", "../escapa", "a/b", "/abs", "C:\\x", "a\0b", "!!!", ""] {
            let res = create_collection(td.path(), nome);
            assert!(res.is_err(), "deveria rejeitar nome {nome:?}");
            // Nada escapou pro pai do tempdir.
            assert!(!td.path().join("..").join("escapa").exists());
        }
    }

    #[test]
    fn create_collection_at_devolve_caminho_e_colecao() {
        let td = TempDir::new().unwrap();
        let parent = td.path().display().to_string();
        let (path, col) = create_collection_at(parent, "Test Col".to_string()).unwrap();
        assert_eq!(path, td.path().join("test-col"));
        assert_eq!(col.name, "Test Col");
        assert!(path.join("collection.yml").is_file());
    }
}
