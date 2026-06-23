// F2 — Estado do app persistido entre sessoes.
//
// Persiste a LISTA de colecoes abertas (caminhos absolutos) num `state.json` em
// `~/.config/ruan/state.json`, pra reabrir tudo no proximo start.
//
// Sem deps novas: o caminho do diretorio de config e montado com `std::env`
// (respeita `XDG_CONFIG_HOME`, com fallback pra `$HOME/.config`). A
// serializacao usa `serde_json` (ja no projeto).
//
// Robustez: ler um state ausente/corrompido NUNCA derruba o app — devolve lista
// vazia. So erros de ESCRITA (gravar) sao propagados, pois ai o usuario perde
// persistencia silenciosamente se ignorarmos.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store::error::StoreError;

pub mod globals;

const APP_DIR: &str = "ruan";
const STATE_FILE: &str = "state.json";

/// Conteudo do `state.json`. Campo unico por enquanto (`open_collections`);
/// estruturado pra crescer (abas, ultima ativa, etc.) sem quebrar formato.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    /// Caminhos absolutos das colecoes abertas, na ordem de exibicao.
    #[serde(default)]
    pub open_collections: Vec<String>,
}

/// Resolve o diretorio de config do app: `$XDG_CONFIG_HOME/ruan` ou
/// `$HOME/.config/ruan`. LOGICA PURA (recebe as env vars como Option),
/// testavel sem tocar o ambiente real.
pub fn config_dir_de(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    // XDG_CONFIG_HOME so vale se for absoluto e nao-vazio (spec XDG).
    if let Some(x) = xdg_config_home {
        let x = x.trim();
        if !x.is_empty() && PathBuf::from(x).is_absolute() {
            return Some(PathBuf::from(x).join(APP_DIR));
        }
    }
    if let Some(h) = home {
        let h = h.trim();
        if !h.is_empty() {
            return Some(PathBuf::from(h).join(".config").join(APP_DIR));
        }
    }
    None
}

/// Le as env vars reais e resolve o diretorio de config do app.
fn config_dir() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    config_dir_de(xdg.as_deref(), home.as_deref())
}

/// Caminho completo do `state.json`, se conseguirmos resolver o diretorio.
fn state_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(STATE_FILE))
}

/// Parseia o conteudo do `state.json`. LOGICA PURA: JSON invalido/vazio nao
/// derruba nada — devolve `AppState::default()`. (Um state corrompido nunca deve
/// impedir o app de abrir.)
pub fn parse_state(json: &str) -> AppState {
    serde_json::from_str(json).unwrap_or_default()
}

/// Serializa o `AppState` pro texto que vai pro disco (JSON identado).
pub fn stringify_state(state: &AppState) -> Result<String, StoreError> {
    serde_json::to_string_pretty(state).map_err(|e| StoreError::Io(e.to_string()))
}

/// Normaliza a lista de caminhos antes de persistir: remove vazios/whitespace e
/// duplicatas, preservando a ordem da primeira ocorrencia. LOGICA PURA.
pub fn normalizar_paths(paths: &[String]) -> Vec<String> {
    let mut vistos: Vec<String> = Vec::new();
    for p in paths {
        let t = p.trim();
        if t.is_empty() {
            continue;
        }
        let t = t.to_string();
        if !vistos.contains(&t) {
            vistos.push(t);
        }
    }
    vistos
}

/// Carrega a lista de colecoes abertas persistida. Tolerante a falha: se o
/// arquivo nao existir ou estiver corrompido, devolve lista vazia (nunca erra).
pub fn load_open_collections() -> Vec<String> {
    let path = match state_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let json = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_state(&json).open_collections
}

/// Grava a lista de colecoes abertas, criando o diretorio de config se preciso.
/// Os caminhos sao normalizados (sem vazios/duplicatas) antes de escrever.
pub fn save_open_collections(paths: Vec<String>) -> Result<(), StoreError> {
    let path = state_path()
        .ok_or_else(|| StoreError::Io("nao foi possivel resolver o diretorio de config".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let state = AppState {
        open_collections: normalizar_paths(&paths),
    };
    let json = stringify_state(&state)?;
    std::fs::write(&path, json)?;
    Ok(())
}

// ---- Comandos IPC ----
//
// Estes #[tauri::command] precisam ser registrados no `invoke_handler` do
// `lib.rs` pela fase de Integracao (ver retorno do agente).

/// Comando IPC: cria uma colecao nova em `parent` com `name`. Retorna a colecao
/// carregada (arvore vazia). Reusa `store::collection_ops::create_collection`.
#[tauri::command]
pub fn create_collection(
    parent: String,
    name: String,
) -> Result<crate::store::models::Collection, StoreError> {
    let parent_path = PathBuf::from(&parent);
    crate::store::collection_ops::create_collection(&parent_path, &name)
}

/// Comando IPC: devolve a lista persistida de colecoes abertas.
#[tauri::command]
pub fn load_open_collections_cmd() -> Vec<String> {
    load_open_collections()
}

/// Comando IPC: persiste a lista de colecoes abertas.
#[tauri::command]
pub fn save_open_collections_cmd(paths: Vec<String>) -> Result<(), StoreError> {
    save_open_collections(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- config_dir_de ----

    #[test]
    fn config_dir_prefere_xdg_absoluto() {
        let d = config_dir_de(Some("/custom/cfg"), Some("/home/u")).unwrap();
        assert_eq!(d, PathBuf::from("/custom/cfg/ruan"));
    }

    #[test]
    fn config_dir_ignora_xdg_relativo_usa_home() {
        // XDG_CONFIG_HOME relativo e invalido pela spec -> cai pro HOME.
        let d = config_dir_de(Some("relativo/x"), Some("/home/u")).unwrap();
        assert_eq!(d, PathBuf::from("/home/u/.config/ruan"));
    }

    #[test]
    fn config_dir_ignora_xdg_vazio_usa_home() {
        let d = config_dir_de(Some("   "), Some("/home/u")).unwrap();
        assert_eq!(d, PathBuf::from("/home/u/.config/ruan"));
    }

    #[test]
    fn config_dir_so_home() {
        let d = config_dir_de(None, Some("/home/u")).unwrap();
        assert_eq!(d, PathBuf::from("/home/u/.config/ruan"));
    }

    #[test]
    fn config_dir_sem_nada_e_none() {
        assert!(config_dir_de(None, None).is_none());
        assert!(config_dir_de(Some(""), Some("")).is_none());
        assert!(config_dir_de(Some("rel"), Some("  ")).is_none());
    }

    // ---- parse_state / stringify_state round-trip ----

    #[test]
    fn parse_state_json_valido() {
        let s = parse_state(r#"{"openCollections":["/a","/b"]}"#);
        assert_eq!(s.open_collections, vec!["/a", "/b"]);
    }

    #[test]
    fn parse_state_vazio_ou_corrompido_vira_default() {
        assert_eq!(parse_state(""), AppState::default());
        assert_eq!(parse_state("{nao json"), AppState::default());
        assert_eq!(parse_state("null"), AppState::default());
        // Campo ausente -> default (lista vazia).
        assert_eq!(parse_state("{}"), AppState::default());
    }

    #[test]
    fn round_trip_state() {
        let st = AppState {
            open_collections: vec!["/x".into(), "/y".into()],
        };
        let json = stringify_state(&st).unwrap();
        assert_eq!(parse_state(&json), st);
        // Confirma o nome camelCase no disco.
        assert!(json.contains("openCollections"));
    }

    // ---- normalizar_paths ----

    #[test]
    fn normalizar_remove_vazios_e_trima() {
        let r = normalizar_paths(&["/a".into(), "  ".into(), "".into(), "  /b  ".into()]);
        assert_eq!(r, vec!["/a", "/b"]);
    }

    #[test]
    fn normalizar_remove_duplicatas_preservando_ordem() {
        let r = normalizar_paths(&["/a".into(), "/b".into(), "/a".into(), "/c".into(), "/b".into()]);
        assert_eq!(r, vec!["/a", "/b", "/c"]);
    }

    #[test]
    fn normalizar_lista_vazia() {
        let r = normalizar_paths(&[]);
        assert!(r.is_empty());
    }
}
