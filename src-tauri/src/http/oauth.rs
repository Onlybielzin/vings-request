// F11 — OAuth2: obtencao de access token via reqwest.
//
// REGISTRAR NO lib.rs (fase de Integracao):
//   http::oauth::oauth2_token
//
// Suporta os grants mais comuns para um cliente HTTP de bancada:
//   - client_credentials: troca client_id/secret por token (server-to-server).
//   - authorization_code:  troca um `code` ja obtido (mais redirect_uri) por
//                          token. O fluxo de browser/redirect que PRODUZ o code
//                          e do front (M3+); aqui so fazemos a troca code->token.
//   - password (ROPC):     username/password trocados por token (legado, mas
//                          alguns servidores ainda usam).
//   - refresh_token:       renova um token a partir de um refresh_token.
//
// Tudo via POST application/x-www-form-urlencoded no token endpoint, conforme
// RFC 6749. A autenticacao do cliente pode ir no corpo (client_id/secret) ou no
// header Basic (client_secret_basic), escolhida por `clientAuth`.
//
// SEGURANCA:
//   - NUNCA logamos client_secret, password, code, tokens ou o corpo da resposta.
//     Em erro, so devolvemos a mensagem do servidor (campo `error`/status), nunca
//     os segredos enviados.
//   - So aceitamos token_url http/https (rejeita file:, data:, etc.).
//   - O token e devolvido ao front, que decide onde guardar (campo `token` da
//     auth oauth2 em memoria). Nao persistimos nada aqui.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Onde colocar as credenciais do cliente na requisicao de token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthMode {
    /// client_id/client_secret no corpo (application/x-www-form-urlencoded).
    Body,
    /// Authorization: Basic base64(client_id:client_secret).
    Basic,
}

impl Default for ClientAuthMode {
    fn default() -> Self {
        ClientAuthMode::Body
    }
}

/// Configuracao para obter um token OAuth2. Campos opcionais por grant.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2Config {
    /// grant: "client_credentials" | "authorization_code" | "password" | "refresh_token".
    pub grant_type: String,
    /// Endpoint do token (http/https obrigatorio).
    pub token_url: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    /// authorization_code: o code obtido + redirect_uri usado.
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// password grant (ROPC).
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// refresh_token grant.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Onde mandar client_id/secret (body padrao, ou Basic).
    #[serde(default)]
    pub client_auth: ClientAuthMode,
    /// Timeout em ms (default 30s).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Token devolvido ao front. `accessToken` e o que importa para o Bearer.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2Token {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    /// Segundos ate expirar (se o servidor informar).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Erro tipado de OAuth2, serializado como {kind, message} pro front. Mensagens
/// NUNCA incluem segredos enviados; no maximo o `error`/status do servidor.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2Error {
    pub kind: String,
    pub message: String,
}

impl OAuth2Error {
    fn novo(kind: &str, message: impl Into<String>) -> Self {
        OAuth2Error {
            kind: kind.to_string(),
            message: message.into(),
        }
    }
}

const TIMEOUT_PADRAO_MS: u64 = 30_000;

/// Resposta crua do token endpoint. Aceita expires_in como numero. Campos extras
/// sao ignorados.
#[derive(Debug, Deserialize)]
struct RespostaToken {
    access_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    // Campos de erro padrao OAuth2 (RFC 6749 5.2).
    error: Option<String>,
    error_description: Option<String>,
}

/// Valida o esquema do token_url (so http/https). LOGICA PURA.
pub fn validar_token_url(url: &str) -> Result<(), OAuth2Error> {
    let u = url.trim();
    if u.is_empty() {
        return Err(OAuth2Error::novo("invalidUrl", "token URL vazia"));
    }
    let parsed = reqwest::Url::parse(u)
        .map_err(|e| OAuth2Error::novo("invalidUrl", e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        outro => Err(OAuth2Error::novo(
            "invalidUrl",
            format!("scheme nao suportado: {outro}"),
        )),
    }
}

/// Monta os pares do corpo (form-urlencoded) conforme o grant. LOGICA PURA.
/// NAO inclui client_secret quando a auth do cliente e Basic (vai no header).
/// Retorna erro se faltar campo obrigatorio do grant.
pub fn montar_form(cfg: &OAuth2Config) -> Result<HashMap<String, String>, OAuth2Error> {
    let mut form: HashMap<String, String> = HashMap::new();
    let grant = cfg.grant_type.trim();
    form.insert("grant_type".to_string(), grant.to_string());

    match grant {
        "client_credentials" => {}
        "authorization_code" => {
            let code = cfg
                .code
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    OAuth2Error::novo("invalidConfig", "authorization_code exige 'code'")
                })?;
            form.insert("code".to_string(), code.to_string());
            if let Some(ru) = cfg.redirect_uri.as_deref().filter(|s| !s.is_empty()) {
                form.insert("redirect_uri".to_string(), ru.to_string());
            }
        }
        "password" => {
            let user = cfg
                .username
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    OAuth2Error::novo("invalidConfig", "password grant exige 'username'")
                })?;
            let pass = cfg.password.as_deref().unwrap_or("");
            form.insert("username".to_string(), user.to_string());
            form.insert("password".to_string(), pass.to_string());
        }
        "refresh_token" => {
            let rt = cfg
                .refresh_token
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    OAuth2Error::novo(
                        "invalidConfig",
                        "refresh_token grant exige 'refreshToken'",
                    )
                })?;
            form.insert("refresh_token".to_string(), rt.to_string());
        }
        outro => {
            return Err(OAuth2Error::novo(
                "invalidConfig",
                format!("grant_type nao suportado: {outro}"),
            ));
        }
    }

    if let Some(scope) = cfg.scope.as_deref().filter(|s| !s.is_empty()) {
        form.insert("scope".to_string(), scope.to_string());
    }

    // Credenciais do cliente no corpo, exceto quando a auth e Basic.
    if cfg.client_auth == ClientAuthMode::Body {
        if let Some(id) = cfg.client_id.as_deref().filter(|s| !s.is_empty()) {
            form.insert("client_id".to_string(), id.to_string());
        }
        if let Some(secret) = cfg.client_secret.as_deref().filter(|s| !s.is_empty()) {
            form.insert("client_secret".to_string(), secret.to_string());
        }
    }

    Ok(form)
}

/// base64 padrao de bytes (para o header Basic). LOGICA PURA, sem deps novas.
fn base64_padrao(bytes: &[u8]) -> String {
    const ALFA: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let trio = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALFA[((trio >> 18) & 0x3f) as usize] as char);
        out.push(ALFA[((trio >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALFA[((trio >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALFA[(trio & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Valor do header Authorization: Basic para client_secret_basic. LOGICA PURA.
pub fn header_basic_cliente(client_id: &str, client_secret: &str) -> String {
    let cred = format!("{client_id}:{client_secret}");
    format!("Basic {}", base64_padrao(cred.as_bytes()))
}

/// Mapeia a resposta crua do servidor em OAuth2Token ou OAuth2Error. LOGICA PURA.
/// `status_ok` indica se o HTTP foi 2xx (alguns servidores devolvem erro com 400).
fn interpretar_resposta(
    status_ok: bool,
    resp: RespostaToken,
) -> Result<OAuth2Token, OAuth2Error> {
    if let Some(err) = resp.error {
        // Erro OAuth2 padrao: devolve o codigo do servidor + descricao (sem
        // jamais ecoar segredos enviados).
        let desc = resp.error_description.unwrap_or_default();
        let msg = if desc.is_empty() {
            err.clone()
        } else {
            format!("{err}: {desc}")
        };
        return Err(OAuth2Error::novo("oauthError", msg));
    }
    match resp.access_token {
        Some(token) if !token.is_empty() => Ok(OAuth2Token {
            access_token: token,
            token_type: resp.token_type,
            expires_in: resp.expires_in,
            refresh_token: resp.refresh_token,
            scope: resp.scope,
        }),
        _ => {
            let dica = if status_ok {
                "resposta sem access_token"
            } else {
                "falha ao obter token (sem access_token na resposta)"
            };
            Err(OAuth2Error::novo("noToken", dica))
        }
    }
}

/// Comando Tauri: obtem um access token OAuth2. Faz HTTP no token endpoint.
/// Registrar no lib.rs (Integracao). Nunca paniqueia; erros viram OAuth2Error.
#[tauri::command]
pub async fn oauth2_token(config: OAuth2Config) -> Result<OAuth2Token, OAuth2Error> {
    validar_token_url(&config.token_url)?;
    let form = montar_form(&config)?;

    let timeout = Duration::from_millis(config.timeout_ms.unwrap_or(TIMEOUT_PADRAO_MS));
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| OAuth2Error::novo("build", e.to_string()))?;

    let mut req = client
        .post(config.token_url.trim())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, HeaderValue::from_static("application/json"))
        .form(&form);

    // client_secret_basic: credenciais no header Basic.
    if config.client_auth == ClientAuthMode::Basic {
        let id = config.client_id.as_deref().unwrap_or("");
        let secret = config.client_secret.as_deref().unwrap_or("");
        let valor = header_basic_cliente(id, secret);
        let hv = HeaderValue::from_str(&valor)
            .map_err(|_| OAuth2Error::novo("build", "credenciais do cliente invalidas"))?;
        req = req.header(AUTHORIZATION, hv);
    }

    let resp = req.send().await.map_err(|e| {
        let kind = if e.is_timeout() {
            "timeout"
        } else if e.is_connect() {
            "connect"
        } else {
            "network"
        };
        // Mensagem do reqwest nao inclui o corpo enviado (sem vazamento de segredo).
        OAuth2Error::novo(kind, e.to_string())
    })?;

    let status_ok = resp.status().is_success();
    let parsed: RespostaToken = resp
        .json()
        .await
        .map_err(|e| OAuth2Error::novo("decode", e.to_string()))?;

    interpretar_resposta(status_ok, parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_base(grant: &str) -> OAuth2Config {
        OAuth2Config {
            grant_type: grant.to_string(),
            token_url: "https://auth.test/token".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
            code: None,
            redirect_uri: None,
            username: None,
            password: None,
            refresh_token: None,
            client_auth: ClientAuthMode::Body,
            timeout_ms: None,
        }
    }

    // ---- validar_token_url ----------------------------------------------

    #[test]
    fn token_url_https_ok() {
        assert!(validar_token_url("https://a.test/token").is_ok());
    }

    #[test]
    fn token_url_http_ok() {
        assert!(validar_token_url("http://a.test/token").is_ok());
    }

    #[test]
    fn token_url_vazia_erro() {
        let e = validar_token_url("   ").unwrap_err();
        assert_eq!(e.kind, "invalidUrl");
    }

    #[test]
    fn token_url_file_rejeitado() {
        let e = validar_token_url("file:///etc/passwd").unwrap_err();
        assert_eq!(e.kind, "invalidUrl");
    }

    #[test]
    fn token_url_invalida_erro() {
        let e = validar_token_url("nao eh url").unwrap_err();
        assert_eq!(e.kind, "invalidUrl");
    }

    // ---- montar_form ----------------------------------------------------

    #[test]
    fn form_client_credentials_inclui_grant_e_credenciais_no_body() {
        let mut cfg = cfg_base("client_credentials");
        cfg.client_id = Some("id".to_string());
        cfg.client_secret = Some("sec".to_string());
        cfg.scope = Some("read write".to_string());
        let f = montar_form(&cfg).unwrap();
        assert_eq!(f.get("grant_type").unwrap(), "client_credentials");
        assert_eq!(f.get("client_id").unwrap(), "id");
        assert_eq!(f.get("client_secret").unwrap(), "sec");
        assert_eq!(f.get("scope").unwrap(), "read write");
    }

    #[test]
    fn form_basic_auth_omite_secret_do_body() {
        let mut cfg = cfg_base("client_credentials");
        cfg.client_id = Some("id".to_string());
        cfg.client_secret = Some("sec".to_string());
        cfg.client_auth = ClientAuthMode::Basic;
        let f = montar_form(&cfg).unwrap();
        // Em Basic, nada de client_id/secret no corpo (vao no header).
        assert!(f.get("client_id").is_none());
        assert!(f.get("client_secret").is_none());
    }

    #[test]
    fn form_authorization_code_exige_code() {
        let cfg = cfg_base("authorization_code");
        let e = montar_form(&cfg).unwrap_err();
        assert_eq!(e.kind, "invalidConfig");
    }

    #[test]
    fn form_authorization_code_com_code_e_redirect() {
        let mut cfg = cfg_base("authorization_code");
        cfg.code = Some("abc".to_string());
        cfg.redirect_uri = Some("https://app/cb".to_string());
        let f = montar_form(&cfg).unwrap();
        assert_eq!(f.get("code").unwrap(), "abc");
        assert_eq!(f.get("redirect_uri").unwrap(), "https://app/cb");
    }

    #[test]
    fn form_password_exige_username() {
        let cfg = cfg_base("password");
        let e = montar_form(&cfg).unwrap_err();
        assert_eq!(e.kind, "invalidConfig");
    }

    #[test]
    fn form_password_inclui_credenciais() {
        let mut cfg = cfg_base("password");
        cfg.username = Some("u".to_string());
        cfg.password = Some("p".to_string());
        let f = montar_form(&cfg).unwrap();
        assert_eq!(f.get("username").unwrap(), "u");
        assert_eq!(f.get("password").unwrap(), "p");
    }

    #[test]
    fn form_refresh_token_exige_refresh() {
        let cfg = cfg_base("refresh_token");
        let e = montar_form(&cfg).unwrap_err();
        assert_eq!(e.kind, "invalidConfig");
    }

    #[test]
    fn form_refresh_token_inclui_token() {
        let mut cfg = cfg_base("refresh_token");
        cfg.refresh_token = Some("rt".to_string());
        let f = montar_form(&cfg).unwrap();
        assert_eq!(f.get("refresh_token").unwrap(), "rt");
    }

    #[test]
    fn form_grant_desconhecido_erro() {
        let cfg = cfg_base("magico");
        let e = montar_form(&cfg).unwrap_err();
        assert_eq!(e.kind, "invalidConfig");
    }

    #[test]
    fn form_scope_vazio_nao_entra() {
        let mut cfg = cfg_base("client_credentials");
        cfg.scope = Some("".to_string());
        let f = montar_form(&cfg).unwrap();
        assert!(f.get("scope").is_none());
    }

    // ---- base64 / header basic ------------------------------------------

    #[test]
    fn base64_user_pass() {
        assert_eq!(base64_padrao(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64_padrao(b"M"), "TQ==");
        assert_eq!(base64_padrao(b"Ma"), "TWE=");
        assert_eq!(base64_padrao(b"Man"), "TWFu");
    }

    #[test]
    fn header_basic_formato() {
        assert_eq!(header_basic_cliente("user", "pass"), "Basic dXNlcjpwYXNz");
    }

    // ---- interpretar_resposta -------------------------------------------

    #[test]
    fn resposta_com_token_ok() {
        let r = RespostaToken {
            access_token: Some("tok".to_string()),
            token_type: Some("Bearer".to_string()),
            expires_in: Some(3600),
            refresh_token: Some("rt".to_string()),
            scope: Some("read".to_string()),
            error: None,
            error_description: None,
        };
        let t = interpretar_resposta(true, r).unwrap();
        assert_eq!(t.access_token, "tok");
        assert_eq!(t.token_type.as_deref(), Some("Bearer"));
        assert_eq!(t.expires_in, Some(3600));
    }

    #[test]
    fn resposta_com_error_oauth() {
        let r = RespostaToken {
            access_token: None,
            token_type: None,
            expires_in: None,
            refresh_token: None,
            scope: None,
            error: Some("invalid_client".to_string()),
            error_description: Some("bad creds".to_string()),
        };
        let e = interpretar_resposta(false, r).unwrap_err();
        assert_eq!(e.kind, "oauthError");
        assert!(e.message.contains("invalid_client"));
        assert!(e.message.contains("bad creds"));
    }

    #[test]
    fn resposta_sem_token_nem_erro() {
        let r = RespostaToken {
            access_token: None,
            token_type: None,
            expires_in: None,
            refresh_token: None,
            scope: None,
            error: None,
            error_description: None,
        };
        let e = interpretar_resposta(true, r).unwrap_err();
        assert_eq!(e.kind, "noToken");
    }

    #[test]
    fn resposta_token_vazio_e_erro() {
        let r = RespostaToken {
            access_token: Some("".to_string()),
            token_type: None,
            expires_in: None,
            refresh_token: None,
            scope: None,
            error: None,
            error_description: None,
        };
        let e = interpretar_resposta(true, r).unwrap_err();
        assert_eq!(e.kind, "noToken");
    }

    #[test]
    fn token_serializa_camel_case_e_omite_none() {
        let t = OAuth2Token {
            access_token: "x".to_string(),
            token_type: None,
            expires_in: None,
            refresh_token: None,
            scope: None,
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["accessToken"], "x");
        assert!(v.get("tokenType").is_none());
        assert!(v.get("expiresIn").is_none());
    }
}
