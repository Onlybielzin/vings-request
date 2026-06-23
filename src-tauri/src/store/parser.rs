// Camada de parse/stringify ISOLADA — LOGICA PURA (sem I/O), alvo de mutation testing.
// Converte entre as structs do modelo e o texto YAML gravado em disco.
// Garantia de round-trip: parse(stringify(x)) == x para os tipos abaixo.

use crate::store::error::StoreError;
use crate::store::models::{CollectionMeta, FolderMeta, RequestItem};

/// Parseia o YAML de uma request (`<slug>.yml`) numa `RequestItem`.
pub fn parse_request(yaml: &str) -> Result<RequestItem, StoreError> {
    let req: RequestItem = serde_yaml::from_str(yaml)?;
    Ok(req)
}

/// Serializa uma `RequestItem` para o texto YAML que vai pro disco.
pub fn stringify_request(req: &RequestItem) -> Result<String, StoreError> {
    let s = serde_yaml::to_string(req)?;
    Ok(s)
}

/// Parseia o `collection.yml` (so metadados; a arvore vem do filesystem).
pub fn parse_collection_meta(yaml: &str) -> Result<CollectionMeta, StoreError> {
    let meta: CollectionMeta = serde_yaml::from_str(yaml)?;
    Ok(meta)
}

/// Serializa os metadados da colecao para o `collection.yml`.
pub fn stringify_collection_meta(meta: &CollectionMeta) -> Result<String, StoreError> {
    let s = serde_yaml::to_string(meta)?;
    Ok(s)
}

/// Parseia o `folder.yml` (so metadados; os filhos vem do filesystem).
pub fn parse_folder_meta(yaml: &str) -> Result<FolderMeta, StoreError> {
    let meta: FolderMeta = serde_yaml::from_str(yaml)?;
    Ok(meta)
}

/// Serializa os metadados da pasta para o `folder.yml`.
pub fn stringify_folder_meta(meta: &FolderMeta) -> Result<String, StoreError> {
    let s = serde_yaml::to_string(meta)?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::models::{
        ApiKeyPlacement, Auth, AuthMode, Body, BodyMode, GraphqlBody, KeyValue, Scripts,
    };

    fn kv(name: &str, value: &str, enabled: bool) -> KeyValue {
        KeyValue {
            name: name.to_string(),
            value: value.to_string(),
            enabled,
            description: None,
        }
    }

    fn req_base() -> RequestItem {
        RequestItem {
            name: "Listar Usuarios".to_string(),
            seq: 3,
            method: "GET".to_string(),
            url: "https://api.exemplo.com/users".to_string(),
            headers: vec![],
            params: vec![],
            body: Body::default(),
            auth: Auth::default(),
            scripts: Scripts::default(),
            tests: String::new(),
            docs: String::new(),
        }
    }

    /// Helper de round-trip: stringify -> parse deve devolver igual.
    fn round_trip_request(req: &RequestItem) {
        let yaml = stringify_request(req).unwrap();
        let back = parse_request(&yaml).unwrap();
        assert_eq!(&back, req, "round-trip falhou para yaml:\n{yaml}");
    }

    // ---- round-trip de RequestItem ----

    #[test]
    fn round_trip_request_minima() {
        round_trip_request(&req_base());
    }

    #[test]
    fn round_trip_todos_os_metodos() {
        for m in &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
            let mut r = req_base();
            r.method = m.to_string();
            round_trip_request(&r);
        }
    }

    #[test]
    fn round_trip_com_headers_e_params() {
        let mut r = req_base();
        r.headers = vec![
            kv("Authorization", "Bearer xyz", true),
            kv("X-Debug", "1", false),
        ];
        r.params = vec![kv("page", "2", true), kv("limit", "50", true)];
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_body_json() {
        let mut r = req_base();
        r.method = "POST".to_string();
        r.body = Body {
            mode: BodyMode::Json,
            raw: Some("{\"a\":1}".to_string()),
            form: vec![],
            graphql: None,
        };
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_body_text_e_xml() {
        for mode in [BodyMode::Text, BodyMode::Xml] {
            let mut r = req_base();
            r.body = Body {
                mode,
                raw: Some("conteudo cru".to_string()),
                form: vec![],
                graphql: None,
            };
            round_trip_request(&r);
        }
    }

    #[test]
    fn round_trip_body_form_urlencoded_e_multipart() {
        for mode in [BodyMode::FormUrlencoded, BodyMode::Multipart] {
            let mut r = req_base();
            r.body = Body {
                mode,
                raw: None,
                form: vec![kv("campo", "valor", true), kv("off", "x", false)],
                graphql: None,
            };
            round_trip_request(&r);
        }
    }

    #[test]
    fn round_trip_body_graphql() {
        let mut r = req_base();
        r.body = Body {
            mode: BodyMode::Graphql,
            raw: None,
            form: vec![],
            graphql: Some(GraphqlBody {
                query: "query { me { id } }".to_string(),
                variables: "{\"x\":1}".to_string(),
            }),
        };
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_auth_basic() {
        let mut r = req_base();
        r.auth = Auth {
            mode: AuthMode::Basic,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
            ..Auth::default()
        };
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_auth_bearer() {
        let mut r = req_base();
        r.auth = Auth {
            mode: AuthMode::Bearer,
            token: Some("tok123".to_string()),
            ..Auth::default()
        };
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_auth_apikey_header_e_query() {
        for placement in [ApiKeyPlacement::Header, ApiKeyPlacement::Query] {
            let mut r = req_base();
            r.auth = Auth {
                mode: AuthMode::Apikey,
                key: Some("X-Api-Key".to_string()),
                value: Some("segredo".to_string()),
                placement: Some(placement),
                ..Auth::default()
            };
            round_trip_request(&r);
        }
    }

    #[test]
    fn round_trip_auth_inherit_e_oauth2() {
        for mode in [AuthMode::Inherit, AuthMode::Oauth2] {
            let mut r = req_base();
            r.auth = Auth {
                mode,
                ..Auth::default()
            };
            round_trip_request(&r);
        }
    }

    #[test]
    fn round_trip_scripts_tests_docs() {
        let mut r = req_base();
        r.scripts = Scripts {
            pre: "console.log('pre')".to_string(),
            post: "console.log('post')".to_string(),
        };
        r.tests = "expect(res.status).toBe(200)".to_string();
        r.docs = "# Documentacao\nlinha".to_string();
        round_trip_request(&r);
    }

    #[test]
    fn round_trip_request_completa() {
        let mut r = req_base();
        r.method = "POST".to_string();
        r.headers = vec![kv("Content-Type", "application/json", true)];
        r.params = vec![kv("q", "termo", true)];
        r.body = Body {
            mode: BodyMode::Json,
            raw: Some("{\"x\":true}".to_string()),
            form: vec![],
            graphql: None,
        };
        r.auth = Auth {
            mode: AuthMode::Bearer,
            token: Some("t".to_string()),
            ..Auth::default()
        };
        r.scripts = Scripts {
            pre: "a".to_string(),
            post: "b".to_string(),
        };
        r.tests = "t".to_string();
        r.docs = "d".to_string();
        round_trip_request(&r);
    }

    // ---- defaults ao parsear YAML minimo ----

    #[test]
    fn parse_request_usa_defaults() {
        // So o campo obrigatorio `name`; todo o resto deve cair nos defaults.
        let req = parse_request("name: Minha Req\n").unwrap();
        assert_eq!(req.name, "Minha Req");
        assert_eq!(req.seq, 0);
        assert_eq!(req.method, "GET"); // default_method
        assert_eq!(req.url, "");
        assert!(req.headers.is_empty());
        assert!(req.params.is_empty());
        assert_eq!(req.body.mode, BodyMode::None); // BodyMode default
        assert_eq!(req.auth.mode, AuthMode::None); // AuthMode default
        assert_eq!(req.scripts.pre, "");
        assert_eq!(req.scripts.post, "");
        assert_eq!(req.tests, "");
        assert_eq!(req.docs, "");
    }

    #[test]
    fn parse_request_keyvalue_enabled_default_true() {
        // KeyValue.enabled deve defaultar para true quando omitido.
        let req = parse_request("name: r\nheaders:\n  - name: H\n").unwrap();
        assert_eq!(req.headers.len(), 1);
        assert!(req.headers[0].enabled, "enabled deveria defaultar para true");
        assert_eq!(req.headers[0].value, "");
    }

    #[test]
    fn parse_request_method_explicito_nao_vira_default() {
        // Garante que o default so se aplica quando ausente (mata mutante de default).
        let req = parse_request("name: r\nmethod: DELETE\n").unwrap();
        assert_eq!(req.method, "DELETE");
    }

    #[test]
    fn parse_request_yaml_invalido_erra() {
        // Falta o campo obrigatorio `name`.
        assert!(matches!(parse_request("seq: 1\n"), Err(StoreError::Yaml(_))));
        // YAML sintaticamente quebrado.
        assert!(matches!(parse_request("name: [a, b\n"), Err(StoreError::Yaml(_))));
        // Tipo errado: seq deveria ser numero.
        assert!(matches!(
            parse_request("name: r\nseq: nao-numero\n"),
            Err(StoreError::Yaml(_))
        ));
    }

    #[test]
    fn stringify_request_omite_campos_vazios() {
        // headers/params vazios e raw=None nao devem aparecer no YAML (skip_serializing_if).
        let yaml = stringify_request(&req_base()).unwrap();
        assert!(!yaml.contains("headers"), "yaml:\n{yaml}");
        assert!(!yaml.contains("params"), "yaml:\n{yaml}");
        assert!(!yaml.contains("raw"), "yaml:\n{yaml}");
    }

    // ---- CollectionMeta ----

    #[test]
    fn round_trip_collection_meta() {
        let meta = CollectionMeta {
            name: "Minha Colecao".to_string(),
            version: "2".to_string(),
            vars: Some(serde_yaml::from_str("base_url: http://x").unwrap()),
            auth: None,
        };
        let yaml = stringify_collection_meta(&meta).unwrap();
        let back = parse_collection_meta(&yaml).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn parse_collection_meta_version_default() {
        let meta = parse_collection_meta("name: C\n").unwrap();
        assert_eq!(meta.name, "C");
        assert_eq!(meta.version, "1"); // default_version
        assert!(meta.vars.is_none());
    }

    #[test]
    fn parse_collection_meta_sem_name_erra() {
        assert!(matches!(
            parse_collection_meta("version: '2'\n"),
            Err(StoreError::Yaml(_))
        ));
    }

    // ---- FolderMeta ----

    #[test]
    fn round_trip_folder_meta() {
        let meta = FolderMeta {
            name: "auth".to_string(),
            seq: 7,
            auth: None,
        };
        let yaml = stringify_folder_meta(&meta).unwrap();
        let back = parse_folder_meta(&yaml).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn parse_folder_meta_seq_default_zero() {
        let meta = parse_folder_meta("name: pasta\n").unwrap();
        assert_eq!(meta.name, "pasta");
        assert_eq!(meta.seq, 0); // seq default
    }

    #[test]
    fn parse_folder_meta_sem_name_erra() {
        assert!(matches!(
            parse_folder_meta("seq: 1\n"),
            Err(StoreError::Yaml(_))
        ));
    }
}
