// F9 — Variaveis GLOBAIS do app (escopo mais fraco da resolucao de templates).
//
// Diferente das variaveis de environment (que vivem dentro de cada colecao),
// as globais sao do APP inteiro e ficam em `~/.config/ruan/globals.yml`
// (mesmo diretorio de config do `state.json`). Servem para valores reusados
// entre colecoes (ex.: um token pessoal, um host de staging).
//
// Robustez: ler um arquivo ausente/corrompido NUNCA derruba o app -> lista
// vazia. So erros de ESCRITA sao propagados (senao o usuario perderia dados
// silenciosamente).
//
// Reusa `config_dir_de` (pub) do modulo pai para resolver o diretorio de config
// respeitando XDG_CONFIG_HOME / HOME.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app_state::config_dir_de;
use crate::store::error::StoreError;

const GLOBALS_FILE: &str = "globals.yml";

/// Variavel global do app. Mesma forma da `Variable` de environment (espelho TS
/// unico no front): name/value/enabled/secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
}

fn default_true() -> bool {
    true
}

/// Envelope gravado no `globals.yml`. Estruturado pra crescer sem quebrar formato.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalVars {
    #[serde(default)]
    pub variables: Vec<Variable>,
}

/// Resolve o diretorio de config do app lendo as env vars reais e delegando a
/// `config_dir_de` (logica pura) do modulo pai.
fn config_dir() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    config_dir_de(xdg.as_deref(), home.as_deref())
}

/// Caminho completo do `globals.yml`, se o diretorio de config for resolvivel.
fn globals_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(GLOBALS_FILE))
}

/// Parseia o conteudo do `globals.yml`. LOGICA PURA: YAML invalido/vazio nao
/// derruba nada -> `GlobalVars::default()` (lista vazia).
pub fn parse_globals(yaml: &str) -> GlobalVars {
    serde_yaml::from_str(yaml).unwrap_or_default()
}

/// Serializa as variaveis globais para o texto YAML que vai pro disco.
pub fn stringify_globals(g: &GlobalVars) -> Result<String, StoreError> {
    serde_yaml::to_string(g).map_err(StoreError::from)
}

/// Carrega as variaveis globais persistidas. Tolerante a falha: arquivo ausente
/// ou corrompido -> lista vazia (nunca erra).
pub fn load_global_vars() -> Vec<Variable> {
    let path = match globals_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let yaml = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_globals(&yaml).variables
}

/// Grava as variaveis globais, criando o diretorio de config se preciso.
pub fn save_global_vars(vars: Vec<Variable>) -> Result<(), StoreError> {
    let path = globals_path()
        .ok_or_else(|| StoreError::Io("nao foi possivel resolver o diretorio de config".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let g = GlobalVars { variables: vars };
    let yaml = stringify_globals(&g)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}

// ---- Comandos IPC ----
//
// Estes #[tauri::command] precisam ser registrados no `invoke_handler` do
// `lib.rs` pela fase de Integracao (ver retorno do agente).

/// Comando IPC: devolve as variaveis globais persistidas.
#[tauri::command]
pub fn load_global_vars_cmd() -> Vec<Variable> {
    load_global_vars()
}

/// Comando IPC: persiste as variaveis globais.
#[tauri::command]
pub fn save_global_vars_cmd(vars: Vec<Variable>) -> Result<(), StoreError> {
    save_global_vars(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, value: &str, enabled: bool, secret: bool) -> Variable {
        Variable {
            name: name.to_string(),
            value: value.to_string(),
            enabled,
            secret,
        }
    }

    // ---- parse / stringify round-trip ----

    #[test]
    fn round_trip_globals() {
        let g = GlobalVars {
            variables: vec![
                var("host", "https://x", true, false),
                var("token", "segredo", true, true),
            ],
        };
        let yaml = stringify_globals(&g).unwrap();
        assert_eq!(parse_globals(&yaml), g);
    }

    #[test]
    fn parse_globals_vazio_ou_corrompido_vira_default() {
        assert_eq!(parse_globals(""), GlobalVars::default());
        assert_eq!(parse_globals("[: nao :] yaml"), GlobalVars::default());
        // Campo ausente -> lista vazia.
        assert_eq!(parse_globals("outro: 1\n"), GlobalVars::default());
    }

    #[test]
    fn parse_globals_preserva_secret() {
        let g = parse_globals("variables:\n  - name: t\n    value: v\n    secret: true\n");
        assert_eq!(g.variables.len(), 1);
        assert!(g.variables[0].secret);
        assert!(g.variables[0].enabled); // default true
    }

    #[test]
    fn variable_enabled_default_true_secret_false() {
        let v: Variable = serde_yaml::from_str("name: X\n").unwrap();
        assert!(v.enabled);
        assert!(!v.secret);
        assert_eq!(v.value, "");
    }

    #[test]
    fn parse_globals_carrega_multiplas_variaveis_em_ordem() {
        // Mata mutante que esvazia/encurta a lista: a ordem e a contagem importam.
        let g = parse_globals(
            "variables:\n  - name: a\n    value: \"1\"\n  - name: b\n    value: \"2\"\n",
        );
        assert_eq!(g.variables.len(), 2);
        assert_eq!(g.variables[0].name, "a");
        assert_eq!(g.variables[0].value, "1");
        assert_eq!(g.variables[1].name, "b");
        assert_eq!(g.variables[1].value, "2");
    }

    #[test]
    fn parse_globals_preserva_enabled_false() {
        // enabled: false deve sobreviver (nao virar o default true).
        let g = parse_globals("variables:\n  - name: x\n    value: v\n    enabled: false\n");
        assert_eq!(g.variables.len(), 1);
        assert!(!g.variables[0].enabled);
    }

    #[test]
    fn round_trip_preserva_enabled_e_secret_juntos() {
        let g = GlobalVars {
            variables: vec![
                var("on_secret", "v1", true, true),
                var("off_plain", "v2", false, false),
            ],
        };
        let de = parse_globals(&stringify_globals(&g).unwrap());
        assert_eq!(de, g);
        assert!(de.variables[0].enabled && de.variables[0].secret);
        assert!(!de.variables[1].enabled && !de.variables[1].secret);
    }

    #[test]
    fn stringify_usa_camel_case() {
        // Garante que o disco usa camelCase (espelho TS bate 1:1).
        let g = GlobalVars {
            variables: vec![var("a", "b", true, true)],
        };
        let yaml = stringify_globals(&g).unwrap();
        assert!(yaml.contains("variables"));
        assert!(yaml.contains("secret"));
    }
}
