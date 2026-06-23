// Painel de autoconfiguracao do MCP — BACKEND.
//
// Este modulo expoe comandos Tauri que ajudam o usuario a registrar o servidor
// MCP `ruan-mcp` (binario gerado por este mesmo crate, ver `[[bin]]` no
// Cargo.toml) em clientes de IA: Claude Code (CLI `claude`) e Claude Desktop
// (arquivo JSON em ~/.config/Claude/claude_desktop_config.json).
//
// SEGURANCA / PRINCIPIOS:
// - NUNCA montamos comando de shell por interpolacao de string. O registro no
//   Claude Code usa `std::process::Command` com argumentos em vetor (sem shell),
//   entao um `binary_path` esquisito NAO vira injecao de comando.
// - Antes de registrar/escrever qualquer coisa, VALIDAMOS que `binary_path`
//   existe no disco (evita gravar lixo na config).
// - O merge na config do Claude Desktop PRESERVA tudo que ja existe: apenas
//   define/atualiza `mcpServers.vings-request`, sem remover outras chaves. Antes
//   de sobrescrever o arquivo, fazemos BACKUP (.bak).
// - A logica pura do merge vive em `merge_desktop_config` (recebe/devolve String),
//   testavel sem tocar disco. Os comandos so fazem o I/O em volta dela.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

/// Nome da chave do servidor dentro de `mcpServers`. Casa com o comando
/// documentado: `claude mcp add vings-request -- <path>`.
const SERVER_KEY: &str = "vings-request";

/// Nome do binario MCP gerado por este crate.
const BIN_NAME: &str = "ruan-mcp";

/// Status best-effort da configuracao do MCP nos clientes suportados.
#[derive(Debug, Serialize)]
pub struct McpStatus {
    /// `claude` (CLI do Claude Code) foi encontrado no PATH.
    pub claude_code_cli_present: bool,
    /// Caminho do config do Claude Desktop (exista ou nao o arquivo).
    pub claude_desktop_config_path: Option<String>,
    /// O config do Claude Desktop ja contem `mcpServers["vings-request"]`.
    pub claude_desktop_configured: bool,
}

/// Resolve o `$HOME` do usuario. None se a variavel nao existir/for vazia.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Caminho esperado da config do Claude Desktop (Linux):
/// `~/.config/Claude/claude_desktop_config.json`.
fn claude_desktop_config_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".config/Claude/claude_desktop_config.json"))
}

/// Diretorio `~/.config/Claude`.
fn claude_desktop_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".config/Claude"))
}

/// Verifica se um programa existe no PATH percorrendo as entradas de `$PATH`.
/// Best-effort: sem PATH ou sem match => false.
fn programa_no_path(programa: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    std::env::split_paths(&path).any(|dir| {
        let cand = dir.join(programa);
        cand.is_file()
    })
}

/// Procura o binario `ruan-mcp` em locais conhecidos, em ordem:
/// 1. Ao lado do executavel atual (caso de app empacotado/instalado).
/// 2. Subindo a partir do executavel atual ate achar uma pasta `target`, e
///    olhando `target/release/ruan-mcp` e `target/debug/ruan-mcp` (caso dev,
///    onde o exe roda de dentro de `target/<perfil>/...`).
/// 3. Via `CARGO_MANIFEST_DIR` (quando definido, ex.: testes/dev): `<manifest>/
///    target/release` e `<manifest>/target/debug`.
///
/// Retorna o primeiro que existir como arquivo, ou None.
fn resolver_binario() -> Option<PathBuf> {
    let mut candidatos: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        // (1) ao lado do executavel atual.
        if let Some(dir) = exe.parent() {
            candidatos.push(dir.join(BIN_NAME));
        }
        // (2) subindo ate achar "target".
        for ancestral in exe.ancestors() {
            if ancestral.file_name().map(|n| n == "target").unwrap_or(false) {
                candidatos.push(ancestral.join("release").join(BIN_NAME));
                candidatos.push(ancestral.join("debug").join(BIN_NAME));
                break;
            }
        }
    }

    // (3) via manifest dir (relativo ao crate src-tauri).
    if let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR") {
        let base = PathBuf::from(manifest).join("target");
        candidatos.push(base.join("release").join(BIN_NAME));
        candidatos.push(base.join("debug").join(BIN_NAME));
    }

    candidatos.into_iter().find(|p| p.is_file())
}

/// Comando Tauri: resolve o caminho do binario `ruan-mcp`.
/// Some(path) se achou; None se nao achou (o front mostra o comando de build).
#[tauri::command]
pub fn mcp_binary_path() -> Result<Option<String>, String> {
    Ok(resolver_binario().map(|p| p.display().to_string()))
}

/// Comando Tauri: status best-effort da configuracao do MCP. Erros de I/O viram
/// flags false em vez de propagar (o painel apenas exibe o estado).
#[tauri::command]
pub fn mcp_setup_status() -> Result<McpStatus, String> {
    let claude_code_cli_present = programa_no_path("claude");

    let config_path = claude_desktop_config_path();
    let claude_desktop_config_path_str = config_path.as_ref().map(|p| p.display().to_string());

    let claude_desktop_configured = config_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|txt| serde_json::from_str::<Value>(&txt).ok())
        .map(|v| {
            v.get("mcpServers")
                .and_then(|m| m.get(SERVER_KEY))
                .is_some()
        })
        .unwrap_or(false);

    Ok(McpStatus {
        claude_code_cli_present,
        claude_desktop_config_path: claude_desktop_config_path_str,
        claude_desktop_configured,
    })
}

/// Valida que `binary_path` aponta para um arquivo existente. Mensagem amigavel.
fn validar_binario(binary_path: &str) -> Result<(), String> {
    if binary_path.trim().is_empty() {
        return Err("Caminho do binario vazio. Compile o ruan-mcp primeiro.".to_string());
    }
    if !Path::new(binary_path).is_file() {
        return Err(format!(
            "Binario nao encontrado em '{binary_path}'. Compile com: cd src-tauri && cargo build --release --bin ruan-mcp"
        ));
    }
    Ok(())
}

/// Resolve o caminho CANONICO do binario `ruan-mcp` que sera gravado nas configs.
///
/// Defense-in-depth: os comandos de registro recebem `binary_path` do frontend
/// apenas como dica, mas NUNCA o gravam diretamente. Aqui re-resolvemos via
/// `resolver_binario()` (locais conhecidos a partir de `current_exe`/manifest) e
/// so aceitamos o input se ele bater com o caminho resolvido. Assim, mesmo que
/// algum JS no webview chame `invoke` com um caminho arbitrario (ex.: `/bin/sh`),
/// nada estranho e escrito como `command` na config do cliente de IA.
fn resolver_binario_canonico(dica: &str) -> Result<String, String> {
    let resolvido = resolver_binario().ok_or_else(|| {
        "Binario ruan-mcp nao encontrado. Compile com: cd src-tauri && cargo build --release --bin ruan-mcp".to_string()
    })?;
    let resolvido_str = resolvido.display().to_string();

    // Aceita a dica do front so se for exatamente o caminho resolvido OU se
    // apontar para o mesmo arquivo no disco (canonicalize). Caso contrario,
    // ignora a dica e usa o valor resolvido localmente.
    let dica = dica.trim();
    if !dica.is_empty() && dica != resolvido_str {
        let mesmo_arquivo = std::fs::canonicalize(dica)
            .ok()
            .zip(std::fs::canonicalize(&resolvido).ok())
            .map(|(a, b)| a == b)
            .unwrap_or(false);
        if !mesmo_arquivo {
            // Dica divergente: nao confia nela, usa o resolvido.
            return Ok(resolvido_str);
        }
    }
    Ok(resolvido_str)
}

/// Comando Tauri: registra o servidor no Claude Code via CLI `claude`.
///
/// Roda `claude mcp add vings-request -- <binary_path>` SEM shell: os argumentos
/// vao num vetor para `std::process::Command`, entao `binary_path` nunca e
/// interpretado como shell (sem risco de injecao).
#[tauri::command]
pub fn mcp_register_claude_code(binary_path: String) -> Result<String, String> {
    // Defense-in-depth: re-resolve o caminho canonico em vez de confiar no input.
    let binary_path = resolver_binario_canonico(&binary_path)?;
    validar_binario(&binary_path)?;

    let saida = std::process::Command::new("claude")
        .args(["mcp", "add", SERVER_KEY, "--", &binary_path])
        .output();

    match saida {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                let msg = format!("{stdout}{stderr}");
                let msg = msg.trim();
                if msg.is_empty() {
                    Ok(format!("Registrado no Claude Code como '{SERVER_KEY}'."))
                } else {
                    Ok(msg.to_string())
                }
            } else {
                Err(format!(
                    "Falha ao registrar no Claude Code (codigo {}). {}{}\n\nVoce pode copiar e rodar manualmente:\nclaude mcp add {SERVER_KEY} -- {binary_path}",
                    out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                    stdout.trim(),
                    stderr.trim(),
                ))
            }
        }
        Err(e) => Err(format!(
            "CLI 'claude' nao encontrado ou nao executou ({e}). Instale o Claude Code, ou copie e rode manualmente:\nclaude mcp add {SERVER_KEY} -- {binary_path}"
        )),
    }
}

/// LOGICA PURA do merge da config do Claude Desktop.
///
/// Recebe o JSON atual como string (`existing`; pode ser vazio => comeca de `{}`)
/// e devolve o novo JSON (pretty) com `mcpServers.vings-request = { "command":
/// binary_path }` definido, PRESERVANDO todas as outras chaves/entradas.
///
/// Erros: JSON existente invalido, ou raiz/`mcpServers` que nao sao objetos.
pub fn merge_desktop_config(existing: &str, binary_path: &str) -> Result<String, String> {
    // Base: objeto vazio se a string for vazia/whitespace; senao parseia.
    let mut root: Value = if existing.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(existing)
            .map_err(|e| format!("Config existente nao e JSON valido: {e}"))?
    };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| "Config raiz deve ser um objeto JSON.".to_string())?;

    // Garante mcpServers como objeto (cria se ausente; erra se for outro tipo).
    let servers_entry = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers_entry
        .as_object_mut()
        .ok_or_else(|| "'mcpServers' existente nao e um objeto JSON.".to_string())?;

    // Define/atualiza apenas a nossa entrada; o resto fica intacto.
    let mut entrada = Map::new();
    entrada.insert("command".to_string(), Value::String(binary_path.to_string()));
    servers.insert(SERVER_KEY.to_string(), Value::Object(entrada));

    serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Falha ao serializar config: {e}"))
}

/// Comando Tauri: faz MERGE na config do Claude Desktop, preservando o resto.
///
/// Passos: valida binario; resolve `~/.config/Claude` (erro amigavel se a pasta
/// nao existir => Claude Desktop nao instalado); le o JSON atual (ou comeca de
/// `{}`); faz BACKUP `.bak` se o arquivo existir; aplica `merge_desktop_config`;
/// grava pretty. Retorna o caminho do config escrito.
#[tauri::command]
pub fn mcp_register_claude_desktop(binary_path: String) -> Result<String, String> {
    // Defense-in-depth: re-resolve o caminho canonico em vez de confiar no input.
    let binary_path = resolver_binario_canonico(&binary_path)?;
    validar_binario(&binary_path)?;

    let dir = claude_desktop_dir()
        .ok_or_else(|| "Nao foi possivel resolver $HOME para localizar o Claude Desktop.".to_string())?;
    if !dir.is_dir() {
        return Err(
            "Claude Desktop nao encontrado (pasta ~/.config/Claude inexistente). Instale e abra o Claude Desktop ao menos uma vez.".to_string(),
        );
    }

    let config_path = dir.join("claude_desktop_config.json");

    // Le o conteudo atual (vazio se o arquivo nao existir ainda).
    let atual = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Falha ao ler config existente: {e}")),
    };

    // Backup antes de sobrescrever, se ja existir.
    if config_path.is_file() {
        let backup = config_path.with_extension("json.bak");
        std::fs::copy(&config_path, &backup)
            .map_err(|e| format!("Falha ao criar backup ({}): {e}", backup.display()))?;
    }

    let novo = merge_desktop_config(&atual, &binary_path)?;

    std::fs::write(&config_path, novo)
        .map_err(|e| format!("Falha ao gravar config ({}): {e}", config_path.display()))?;

    Ok(config_path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_em_config_vazia_cria_estrutura() {
        let out = merge_desktop_config("", "/bin/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["mcpServers"][SERVER_KEY]["command"],
            Value::String("/bin/ruan-mcp".into())
        );
    }

    #[test]
    fn merge_em_objeto_vazio_literal() {
        let out = merge_desktop_config("{}", "/x/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/x/ruan-mcp");
    }

    #[test]
    fn merge_preserva_outras_chaves_e_outros_servers() {
        let existing = r#"{
            "theme": "dark",
            "mcpServers": {
                "outro": { "command": "/usr/bin/outro", "args": ["--flag"] }
            }
        }"#;
        let out = merge_desktop_config(existing, "/opt/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        // Preserva chave de topo.
        assert_eq!(v["theme"], "dark");
        // Preserva o outro server intacto.
        assert_eq!(v["mcpServers"]["outro"]["command"], "/usr/bin/outro");
        assert_eq!(v["mcpServers"]["outro"]["args"][0], "--flag");
        // Adiciona o nosso.
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/opt/ruan-mcp");
    }

    #[test]
    fn merge_atualiza_entrada_existente_sem_duplicar() {
        let existing = r#"{ "mcpServers": { "vings-request": { "command": "/velho" } } }"#;
        let out = merge_desktop_config(existing, "/novo/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/novo/ruan-mcp");
        // Continua sendo uma unica entrada.
        assert_eq!(v["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn merge_json_invalido_erra() {
        assert!(merge_desktop_config("{ nao json", "/x").is_err());
    }

    #[test]
    fn merge_raiz_nao_objeto_erra() {
        assert!(merge_desktop_config("[1,2,3]", "/x").is_err());
    }

    #[test]
    fn merge_mcpservers_nao_objeto_erra() {
        let existing = r#"{ "mcpServers": "oops" }"#;
        assert!(merge_desktop_config(existing, "/x").is_err());
    }

    #[test]
    fn validar_binario_inexistente_erra() {
        assert!(validar_binario("/caminho/que/nao/existe/ruan-mcp").is_err());
        assert!(validar_binario("").is_err());
    }

    // --- Casos pedidos pelo agente de testes (logica pura merge_desktop_config) ---

    #[test]
    fn merge_vazio_cria_command_aninhado() {
        // JSON vazio/"" cria mcpServers.vings-request.command com o caminho.
        for entrada in ["", "  ", "{}"] {
            let out = merge_desktop_config(entrada, "/bin/ruan-mcp").unwrap();
            let v: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(
                v["mcpServers"][SERVER_KEY]["command"], "/bin/ruan-mcp",
                "falhou para entrada {entrada:?}"
            );
        }
    }

    #[test]
    fn merge_preserva_chave_top_level_global_shortcut() {
        // Config com outra chave de topo (globalShortcut) deve preserva-la.
        let existing = r#"{ "globalShortcut": "Ctrl+Alt+Space" }"#;
        let out = merge_desktop_config(existing, "/opt/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        // A chave de topo continua intacta.
        assert_eq!(v["globalShortcut"], "Ctrl+Alt+Space");
        // E o nosso server foi adicionado.
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/opt/ruan-mcp");
    }

    #[test]
    fn merge_preserva_outros_servers_e_adiciona_o_nosso() {
        // Config com OUTROS mcpServers preserva-os e adiciona vings-request.
        let existing = r#"{
            "mcpServers": {
                "filesystem": { "command": "/usr/bin/fs-mcp", "args": ["/data"] },
                "git": { "command": "/usr/bin/git-mcp" }
            }
        }"#;
        let out = merge_desktop_config(existing, "/opt/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let servers = v["mcpServers"].as_object().unwrap();
        // Os dois originais + o nosso = 3.
        assert_eq!(servers.len(), 3);
        assert_eq!(v["mcpServers"]["filesystem"]["command"], "/usr/bin/fs-mcp");
        assert_eq!(v["mcpServers"]["filesystem"]["args"][0], "/data");
        assert_eq!(v["mcpServers"]["git"]["command"], "/usr/bin/git-mcp");
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/opt/ruan-mcp");
    }

    #[test]
    fn merge_json_malformado_nao_perde_dados() {
        // JSON malformado -> Err. Quem chama nao sobrescreve (e mantem o .bak).
        let r = merge_desktop_config("{ \"mcpServers\": ", "/x/ruan-mcp");
        assert!(r.is_err());
    }

    #[test]
    fn merge_idempotente_atualiza_command_sem_duplicar() {
        // Re-merge: rodar duas vezes nao duplica a entrada; so atualiza command.
        let primeiro = merge_desktop_config("{}", "/v1/ruan-mcp").unwrap();
        let segundo = merge_desktop_config(&primeiro, "/v2/ruan-mcp").unwrap();
        let v: Value = serde_json::from_str(&segundo).unwrap();
        let servers = v["mcpServers"].as_object().unwrap();
        // Continua sendo uma unica entrada (sem duplicar a chave).
        assert_eq!(servers.len(), 1);
        // E o command foi atualizado para o novo valor.
        assert_eq!(v["mcpServers"][SERVER_KEY]["command"], "/v2/ruan-mcp");
        // Re-merge com o MESMO caminho e estavel (idempotencia real).
        let terceiro = merge_desktop_config(&segundo, "/v2/ruan-mcp").unwrap();
        assert_eq!(terceiro, segundo);
    }
}
