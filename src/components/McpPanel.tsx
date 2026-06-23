// Painel lateral "Conectar IA / MCP" — autoconfiguracao do servidor MCP.
//
// Componente FINO: so orquestra a UI e chama os comandos do backend
// (src-tauri/src/mcp_setup.rs) via wrappers `invoke` no topo do arquivo, no
// mesmo padrao dos outros componentes. NAO ha logica de path/merge aqui — isso
// vive no Rust (puro/testavel). Aqui apenas resolvemos o caminho do binario,
// disparamos os registros e exibimos sucesso/erro.
//
// Integracao (App.tsx) deve montar <McpPanel /> num painel lateral, aberto por
// um botao "IA" / "MCP" no header (mesmo padrao de "Variaveis", "Importar/
// Exportar", "Settings").

import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";

// --- Wrappers IPC (1:1 com os #[tauri::command] de mcp_setup.rs) -------------

interface McpStatus {
  claude_code_cli_present: boolean;
  claude_desktop_config_path: string | null;
  claude_desktop_configured: boolean;
}

function ipcMcpBinaryPath(): Promise<string | null> {
  return invoke<string | null>("mcp_binary_path");
}

function ipcMcpSetupStatus(): Promise<McpStatus> {
  return invoke<McpStatus>("mcp_setup_status");
}

function ipcMcpRegisterClaudeCode(binaryPath: string): Promise<string> {
  return invoke<string>("mcp_register_claude_code", { binaryPath });
}

function ipcMcpRegisterClaudeDesktop(binaryPath: string): Promise<string> {
  return invoke<string>("mcp_register_claude_desktop", { binaryPath });
}

// --- UI ----------------------------------------------------------------------

const BUILD_CMD = "cd src-tauri && cargo build --release --bin ruan-mcp";

/** Copia texto para o clipboard, com fallback de selecao via textarea oculto. */
async function copiar(texto: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(texto);
      return true;
    }
  } catch {
    // cai no fallback
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = texto;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

export function McpPanel() {
  const [binaryPath, setBinaryPath] = useState<string | null>(null);
  const [carregando, setCarregando] = useState(true);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [info, setInfo] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [copiado, setCopiado] = useState(false);

  async function recarregar() {
    setCarregando(true);
    try {
      const [path, st] = await Promise.all([
        ipcMcpBinaryPath(),
        ipcMcpSetupStatus(),
      ]);
      setBinaryPath(path);
      setStatus(st);
    } catch (e) {
      setErro(String(e));
    } finally {
      setCarregando(false);
    }
  }

  useEffect(() => {
    void recarregar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const comandoClaude = binaryPath
    ? `claude mcp add vings-request -- ${binaryPath}`
    : "";

  function limpar() {
    setInfo(null);
    setErro(null);
  }

  async function configurarClaudeCode() {
    limpar();
    if (!binaryPath) return;
    setBusy(true);
    try {
      const msg = await ipcMcpRegisterClaudeCode(binaryPath);
      setInfo(msg || "Registrado no Claude Code.");
      await recarregar();
    } catch (e) {
      setErro(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function configurarClaudeDesktop() {
    limpar();
    if (!binaryPath) return;
    setBusy(true);
    try {
      const escrito = await ipcMcpRegisterClaudeDesktop(binaryPath);
      setInfo(
        `Config do Claude Desktop atualizada em ${escrito}. Reinicie o Claude Desktop para aplicar.`,
      );
      await recarregar();
    } catch (e) {
      setErro(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function copiarComando() {
    const alvo = comandoClaude || BUILD_CMD;
    const ok = await copiar(alvo);
    setCopiado(ok);
    if (ok) {
      window.setTimeout(() => setCopiado(false), 1500);
    } else {
      setErro("Nao foi possivel copiar. Selecione o texto manualmente.");
    }
  }

  return (
    <div className="mcp-panel" style={estilos.container}>
      <h3 style={estilos.titulo}>Conectar IA / MCP</h3>

      <p style={estilos.intro}>
        O ruan expoe um servidor MCP que deixa uma IA criar e editar colecoes,
        pastas e requests diretamente no app. Configure-o no seu cliente (Claude
        Code ou Claude Desktop) com um clique abaixo.
      </p>

      {carregando ? (
        <div style={estilos.muted}>Verificando ambiente...</div>
      ) : null}

      {/* Caminho do binario ou instrucao de build */}
      {!carregando && !binaryPath ? (
        <div style={estilos.bloco}>
          <div style={estilos.aviso}>
            Binario ruan-mcp nao encontrado — compile primeiro:
          </div>
          <div style={estilos.linhaCodigo}>
            <code style={estilos.codigo}>{BUILD_CMD}</code>
            <button
              type="button"
              style={estilos.botaoPeq}
              onClick={() => void copiar(BUILD_CMD)}
            >
              Copiar
            </button>
          </div>
          <button
            type="button"
            style={estilos.botao}
            onClick={() => void recarregar()}
          >
            Verificar de novo
          </button>
        </div>
      ) : null}

      {!carregando && binaryPath ? (
        <>
          <div style={estilos.campo}>
            <span style={estilos.label}>Binario ruan-mcp</span>
            <input
              type="text"
              readOnly
              value={binaryPath}
              style={estilos.input}
              aria-label="Caminho do binario ruan-mcp"
              onFocus={(e) => e.currentTarget.select()}
            />
          </div>

          {/* Status atual */}
          {status ? (
            <div style={estilos.statusBox}>
              <div style={estilos.statusLinha}>
                <span>Claude Code (CLI claude)</span>
                <span style={estilos.muted}>
                  {status.claude_code_cli_present
                    ? "CLI detectado"
                    : "CLI nao encontrado no PATH"}
                </span>
              </div>
              <div style={estilos.statusLinha}>
                <span>Claude Desktop</span>
                <span style={estilos.muted}>
                  {status.claude_desktop_configured
                    ? "ja configurado"
                    : "nao configurado"}
                </span>
              </div>
            </div>
          ) : null}

          {/* Botoes de registro */}
          <div style={estilos.botoes}>
            <button
              type="button"
              style={estilos.botao}
              onClick={() => void configurarClaudeCode()}
              disabled={busy}
            >
              Configurar Claude Code
            </button>
            <button
              type="button"
              style={estilos.botao}
              onClick={() => void configurarClaudeDesktop()}
              disabled={busy}
            >
              Configurar Claude Desktop
            </button>
          </div>

          {/* Comando manual + copiar */}
          <div style={estilos.campo}>
            <span style={estilos.label}>
              Ou registre manualmente (Claude Code):
            </span>
            <div style={estilos.linhaCodigo}>
              <input
                type="text"
                readOnly
                value={comandoClaude}
                style={{ ...estilos.input, fontFamily: "monospace" }}
                aria-label="Comando claude mcp add"
                onFocus={(e) => e.currentTarget.select()}
              />
              <button
                type="button"
                style={estilos.botaoPeq}
                onClick={() => void copiarComando()}
              >
                {copiado ? "Copiado" : "Copiar"}
              </button>
            </div>
          </div>
        </>
      ) : null}

      <div style={estilos.docs}>
        Detalhes e formatos de config em <code style={estilos.codigoInline}>docs/MCP.md</code>.
      </div>

      {erro ? (
        <div style={estilos.erro} role="alert">
          {erro}
        </div>
      ) : null}
      {info ? <div style={estilos.ok}>{info}</div> : null}
    </div>
  );
}

const estilos: Record<string, CSSProperties> = {
  container: {
    width: "100%",
    display: "flex",
    flexDirection: "column",
    gap: "0.7rem",
  },
  titulo: { margin: 0, fontSize: "0.95rem", color: "#e6e8ea" },
  intro: { margin: 0, fontSize: "0.8rem", lineHeight: 1.45, color: "#9aa0a6" },
  bloco: { display: "flex", flexDirection: "column", gap: "0.5rem" },
  campo: { display: "flex", flexDirection: "column", gap: "0.25rem" },
  label: { fontSize: "0.8rem", color: "#9aa0a6" },
  input: {
    flex: 1,
    padding: "0.35rem 0.5rem",
    fontSize: "0.8rem",
    background: "#1c1f24",
    color: "#e6e8ea",
    border: "1px solid #2b2f36",
    borderRadius: "4px",
    width: "100%",
  },
  statusBox: {
    display: "flex",
    flexDirection: "column",
    gap: "0.3rem",
    padding: "0.5rem",
    background: "#1c1f24",
    border: "1px solid #2b2f36",
    borderRadius: "4px",
  },
  statusLinha: {
    display: "flex",
    justifyContent: "space-between",
    gap: "0.5rem",
    fontSize: "0.8rem",
    color: "#cdd0d4",
  },
  botoes: { display: "flex", gap: "0.5rem", flexWrap: "wrap" },
  botao: {
    alignSelf: "flex-start",
    padding: "0.4rem 0.7rem",
    fontSize: "0.8rem",
    background: "#2b2f36",
    color: "#e6e8ea",
    border: "1px solid #3a3f47",
    borderRadius: "4px",
    cursor: "pointer",
  },
  botaoPeq: {
    padding: "0.3rem 0.55rem",
    fontSize: "0.75rem",
    background: "#2b2f36",
    color: "#e6e8ea",
    border: "1px solid #3a3f47",
    borderRadius: "4px",
    cursor: "pointer",
    whiteSpace: "nowrap",
  },
  linhaCodigo: { display: "flex", gap: "0.4rem", alignItems: "center" },
  codigo: {
    flex: 1,
    padding: "0.35rem 0.5rem",
    fontSize: "0.78rem",
    fontFamily: "monospace",
    background: "#1c1f24",
    color: "#e6e8ea",
    border: "1px solid #2b2f36",
    borderRadius: "4px",
    overflowX: "auto",
    whiteSpace: "nowrap",
  },
  codigoInline: { fontFamily: "monospace", color: "#cdd0d4" },
  aviso: { fontSize: "0.8rem", color: "#e0b341" },
  muted: { fontSize: "0.78rem", color: "#9aa0a6" },
  docs: { fontSize: "0.78rem", color: "#9aa0a6" },
  erro: { fontSize: "0.78rem", color: "#f48771" },
  ok: { fontSize: "0.78rem", color: "#7bc47f", whiteSpace: "pre-wrap" },
};

export default McpPanel;
