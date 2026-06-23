// F9 — Environments e variaveis por ambiente (escopo do environment ativo).
//
// Layout em disco (dentro do diretorio da colecao):
//   minha-colecao/
//     collection.yml
//     environments/
//       producao.yml      <- Environment serializado (name + variables[])
//       local.yml
//
// Cada arquivo `<slug(name)>.yml` guarda um `Environment`. A pasta
// `environments/` pode nao existir (colecao sem ambientes) -> tratamos como
// lista vazia. Toda escrita passa por `slug_seguro` + `dentro_de` (reusados do
// fs_store), garantindo que nada escape do diretorio da colecao.
//
// Variaveis `secret`: o backend NAO trata o valor de forma especial (persiste
// como qualquer outra). O mascaramento e responsabilidade da UI; aqui apenas
// carregamos/gravamos fielmente o campo `secret` para o front decidir.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::fs;

use serde::{Deserialize, Serialize};

use crate::store::error::StoreError;
use crate::store::fs_store::{dentro_de, MAX_YAML_BYTES};
use crate::store::slug::slug_seguro;

/// Le um `.yml` aplicando o limite `MAX_YAML_BYTES` (defesa contra DoS). Espelha
/// a leitura limitada do `fs_store`: `take` garante que nunca alocamos mais que
/// o limite + 1 byte; arquivos acima do limite sao rejeitados ANTES do parse.
fn ler_yaml_limitado(path: &Path) -> Result<String, StoreError> {
    let file = File::open(path)?;
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

const ENV_DIR: &str = "environments";
const YML_EXT: &str = "yml";

/// Uma variavel de ambiente. `secret` marca valores sensiveis (token, senha);
/// o backend persiste normalmente, o front mascara na UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Se false, a variavel existe no arquivo mas nao participa da resolucao.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Marca valor sensivel (mascarado na UI; nunca logar).
    #[serde(default)]
    pub secret: bool,
}

fn default_true() -> bool {
    true
}

/// Um ambiente nomeado com sua lista de variaveis. Gravado em
/// `environments/<slug(name)>.yml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: Vec<Variable>,
}

/// Caminho da pasta `environments/` dentro da colecao.
fn env_dir(collection_dir: &Path) -> PathBuf {
    collection_dir.join(ENV_DIR)
}

/// Carrega todos os environments de uma colecao, lendo `environments/*.yml`.
/// Tolera a pasta ausente (-> vazio). Aplica o limite de tamanho via
/// `ler_yaml_limitado`. Arquivos que nao parseiam sao ignorados (um ambiente
/// corrompido nao deve impedir os demais de carregar). Ordena por nome para um
/// resultado deterministico.
pub fn load_environments(collection_dir: &Path) -> Result<Vec<Environment>, StoreError> {
    let dir = env_dir(collection_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut envs: Vec<Environment> = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some(YML_EXT) {
            continue;
        }
        let yaml = ler_yaml_limitado(&path)?;
        // Um ambiente corrompido nao derruba os demais.
        if let Ok(env) = serde_yaml::from_str::<Environment>(&yaml) {
            envs.push(env);
        }
    }
    envs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(envs)
}

/// Grava um environment em `environments/<slug(name)>.yml`. O nome e sanitizado
/// antes de virar nome de arquivo; o alvo e confirmado dentro da colecao.
pub fn save_environment(
    collection_dir: &Path,
    env: &Environment,
) -> Result<PathBuf, StoreError> {
    let slug = slug_seguro(&env.name)?;
    let dir = env_dir(collection_dir);
    let alvo = dir.join(format!("{slug}.{YML_EXT}"));
    dentro_de(collection_dir, &alvo)?;

    // TOCTOU conhecido/aceito (ver doc de `dentro_de`): escopo local single-user.
    fs::create_dir_all(&dir)?;
    let yaml = serde_yaml::to_string(env)?;
    fs::write(&alvo, yaml)?;
    Ok(alvo)
}

/// Remove o environment de nome `name` (sanitizado). Idempotente: se o arquivo
/// nao existir, retorna Ok.
pub fn delete_environment(collection_dir: &Path, name: &str) -> Result<(), StoreError> {
    let slug = slug_seguro(name)?;
    let alvo = env_dir(collection_dir).join(format!("{slug}.{YML_EXT}"));
    dentro_de(collection_dir, &alvo)?;
    if alvo.is_file() {
        fs::remove_file(&alvo)?;
    }
    Ok(())
}

// ---- Comandos IPC ----
//
// Estes #[tauri::command] precisam ser registrados no `invoke_handler` do
// `lib.rs` pela fase de Integracao (ver retorno do agente).

/// Comando IPC: lista os environments da colecao em `collection_dir`.
#[tauri::command]
pub fn list_environments(collection_dir: String) -> Result<Vec<Environment>, StoreError> {
    load_environments(&PathBuf::from(collection_dir))
}

/// Comando IPC: grava (cria/atualiza) um environment na colecao.
#[tauri::command]
pub fn save_environment_cmd(
    collection_dir: String,
    env: Environment,
) -> Result<(), StoreError> {
    save_environment(&PathBuf::from(collection_dir), &env)?;
    Ok(())
}

/// Comando IPC: remove um environment pelo nome.
#[tauri::command]
pub fn delete_environment_cmd(
    collection_dir: String,
    name: String,
) -> Result<(), StoreError> {
    delete_environment(&PathBuf::from(collection_dir), &name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::models::CollectionMeta;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn var(name: &str, value: &str, enabled: bool, secret: bool) -> Variable {
        Variable {
            name: name.to_string(),
            value: value.to_string(),
            enabled,
            secret,
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
            auth: None,
        };
        crate::store::fs_store::save_collection_meta(&dir, &meta).unwrap();
        (td, dir)
    }

    // ---- load_environments ----

    #[test]
    fn load_environments_pasta_ausente_vira_vazio() {
        let (_td, dir) = col_temp();
        // Sem pasta environments/ -> lista vazia, sem erro.
        assert!(load_environments(&dir).unwrap().is_empty());
    }

    #[test]
    fn save_e_load_round_trip() {
        let (_td, dir) = col_temp();
        let env = Environment {
            name: "Producao".to_string(),
            variables: vec![
                var("baseUrl", "https://api.x", true, false),
                var("token", "segredo", true, true),
                var("desligada", "v", false, false),
            ],
        };
        let alvo = save_environment(&dir, &env).unwrap();
        assert_eq!(alvo.file_name().unwrap(), "producao.yml"); // slug

        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0], env);
        // O campo secret sobrevive ao round-trip.
        assert!(lidos[0].variables[1].secret);
        assert!(!lidos[0].variables[0].secret);
    }

    #[test]
    fn load_environments_ordena_por_nome() {
        let (_td, dir) = col_temp();
        for nome in &["Zeta", "Alpha", "Meio"] {
            save_environment(
                &dir,
                &Environment {
                    name: nome.to_string(),
                    variables: vec![],
                },
            )
            .unwrap();
        }
        let nomes: Vec<String> = load_environments(&dir)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(nomes, vec!["Alpha", "Meio", "Zeta"]);
    }

    #[test]
    fn load_environments_ignora_arquivo_corrompido() {
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "bom".to_string(),
                variables: vec![var("a", "1", true, false)],
            },
        )
        .unwrap();
        // Arquivo .yml que nao e um Environment valido.
        fs::write(env_dir(&dir).join("ruim.yml"), "[: nao :] yaml").unwrap();
        let lidos = load_environments(&dir).unwrap();
        // So o bom carrega; o corrompido e ignorado.
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "bom");
    }

    #[test]
    fn load_environments_ignora_nao_yml() {
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "real".to_string(),
                variables: vec![],
            },
        )
        .unwrap();
        fs::write(env_dir(&dir).join("notas.txt"), "ignorar").unwrap();
        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "real");
    }

    #[test]
    fn variable_enabled_default_true() {
        // Omitir enabled no YAML -> default true; secret default false.
        let v: Variable = serde_yaml::from_str("name: X\nvalue: y\n").unwrap();
        assert!(v.enabled);
        assert!(!v.secret);
        assert_eq!(v.value, "y");
    }

    // ---- save_environment: atualiza no lugar ----

    #[test]
    fn save_environment_sobrescreve_mesmo_nome() {
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "Dev".to_string(),
                variables: vec![var("a", "1", true, false)],
            },
        )
        .unwrap();
        save_environment(
            &dir,
            &Environment {
                name: "Dev".to_string(),
                variables: vec![var("a", "2", true, false)],
            },
        )
        .unwrap();
        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].variables[0].value, "2");
    }

    // ---- delete_environment ----

    #[test]
    fn delete_environment_remove_arquivo() {
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "Temp".to_string(),
                variables: vec![],
            },
        )
        .unwrap();
        assert_eq!(load_environments(&dir).unwrap().len(), 1);
        delete_environment(&dir, "Temp").unwrap();
        assert!(load_environments(&dir).unwrap().is_empty());
    }

    #[test]
    fn delete_environment_inexistente_e_idempotente() {
        let (_td, dir) = col_temp();
        // Nome valido, arquivo nao existe -> Ok.
        assert!(delete_environment(&dir, "nao-existe").is_ok());
    }

    // ---- SEGURANCA: nomes maliciosos rejeitados, nada vaza ----

    #[test]
    fn save_environment_nome_malicioso_e_rejeitado() {
        let (_td, dir) = col_temp();
        for nome in &["..", "a/b", "/etc/passwd", "C:\\x", "a\0b"] {
            let res = save_environment(
                &dir,
                &Environment {
                    name: nome.to_string(),
                    variables: vec![],
                },
            );
            assert!(res.is_err(), "deveria rejeitar nome {nome:?}");
        }
    }

    #[test]
    fn delete_environment_nome_malicioso_e_rejeitado() {
        let (_td, dir) = col_temp();
        for nome in &["..", "a/b", "/etc/passwd"] {
            assert!(
                delete_environment(&dir, nome).is_err(),
                "deveria rejeitar {nome:?}"
            );
        }
    }

    #[test]
    fn nada_escrito_fora_da_colecao_em_nome_malicioso() {
        let (_td, dir) = col_temp();
        let pai = dir.parent().unwrap();
        let antes: Vec<_> = fs::read_dir(pai)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        let _ = save_environment(
            &dir,
            &Environment {
                name: "../../escapou".to_string(),
                variables: vec![],
            },
        );
        let depois: Vec<_> = fs::read_dir(pai)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        assert_eq!(antes.len(), depois.len());
    }

    // ---- limite de tamanho ----

    #[test]
    fn load_environments_ignora_subdiretorio() {
        // Um subdiretorio chamado "x.yml" dentro de environments/ nao deve ser
        // tratado como arquivo (mata mutante que ignore o check is_file).
        let (_td, dir) = col_temp();
        let edir = env_dir(&dir);
        fs::create_dir_all(edir.join("fake.yml")).unwrap();
        save_environment(
            &dir,
            &Environment {
                name: "real".to_string(),
                variables: vec![],
            },
        )
        .unwrap();
        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "real");
    }

    #[test]
    fn save_environment_persiste_variavel_desabilitada() {
        // enabled=false deve sobreviver ao disco (nao virar default true).
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "dev".to_string(),
                variables: vec![var("a", "1", false, false)],
            },
        )
        .unwrap();
        let lidos = load_environments(&dir).unwrap();
        assert!(!lidos[0].variables[0].enabled);
    }

    #[test]
    fn delete_environment_nome_que_sanitiza_para_vazio_retorna_invalid_name() {
        // Contrato documentado (achado [BAIXO] da revisao): um nome so de
        // simbolos vira slug vazio -> InvalidName, e NAO no-op. Lock do contrato.
        let (_td, dir) = col_temp();
        let res = delete_environment(&dir, "!!!");
        assert!(matches!(res, Err(StoreError::InvalidName(_))));
    }

    #[test]
    fn list_environments_cmd_le_do_disco() {
        // O comando IPC delega a load_environments com o path como String.
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "viaCmd".to_string(),
                variables: vec![],
            },
        )
        .unwrap();
        let lidos = list_environments(dir.to_string_lossy().into_owned()).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "viaCmd");
    }

    #[test]
    fn load_environments_acima_do_limite_e_rejeitado() {
        let (_td, dir) = col_temp();
        let edir = env_dir(&dir);
        fs::create_dir_all(&edir).unwrap();
        let path = edir.join("gigante.yml");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"name: grande\nvariables: []\nlixo: \"").unwrap();
        let chunk = vec![b'a'; 1024 * 1024];
        for _ in 0..11 {
            f.write_all(&chunk).unwrap();
        }
        f.write_all(b"\"\n").unwrap();
        f.flush().unwrap();
        drop(f);
        assert!(matches!(
            load_environments(&dir),
            Err(StoreError::ArquivoMuitoGrande(_))
        ));
    }

    #[test]
    fn load_environments_exatamente_no_limite_e_aceito() {
        // Arquivo com EXATAMENTE MAX_YAML_BYTES bytes deve ser aceito (limite
        // inclusivo). Mata o mutante que troca `>` por `>=` em ler_yaml_limitado.
        let (_td, dir) = col_temp();
        let edir = env_dir(&dir);
        fs::create_dir_all(&edir).unwrap();
        let path = edir.join("limite.yml");

        // Cabecalho YAML valido que parseia para um Environment vazio, depois
        // uma linha de comentario `#` preenchida ate bater MAX exato. Comentario
        // e ignorado pelo parser, entao o Environment continua valido.
        let header = b"name: limite\nvariables: []\n#";
        let total = MAX_YAML_BYTES as usize;
        assert!(header.len() < total);
        let mut data = Vec::with_capacity(total);
        data.extend_from_slice(header);
        data.resize(total, b'a');
        assert_eq!(data.len() as u64, MAX_YAML_BYTES);
        fs::write(&path, &data).unwrap();

        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "limite");
    }

    #[test]
    fn save_environment_cmd_grava_no_disco() {
        // O comando IPC deve realmente persistir (mata o mutante que troca o
        // corpo por Ok(()) sem gravar).
        let (_td, dir) = col_temp();
        let env = Environment {
            name: "viaCmd".to_string(),
            variables: vec![var("a", "1", true, false)],
        };
        save_environment_cmd(dir.to_string_lossy().into_owned(), env).unwrap();
        let lidos = load_environments(&dir).unwrap();
        assert_eq!(lidos.len(), 1);
        assert_eq!(lidos[0].name, "viaCmd");
        assert_eq!(lidos[0].variables[0].value, "1");
    }

    #[test]
    fn delete_environment_cmd_remove_do_disco() {
        // O comando IPC deve remover de fato (mata o mutante Ok(()) sem efeito).
        let (_td, dir) = col_temp();
        save_environment(
            &dir,
            &Environment {
                name: "Temp".to_string(),
                variables: vec![],
            },
        )
        .unwrap();
        assert_eq!(load_environments(&dir).unwrap().len(), 1);
        delete_environment_cmd(dir.to_string_lossy().into_owned(), "Temp".to_string())
            .unwrap();
        assert!(load_environments(&dir).unwrap().is_empty());
    }
}
