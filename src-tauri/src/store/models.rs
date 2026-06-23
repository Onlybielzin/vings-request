// Schema serializavel do modelo de dados file-based (espelho em src/lib/types.ts).
// Tudo aqui e POJO puro com serde — sem I/O. As structs sao gravadas/lidas como YAML.
//
// Convencoes de serde:
// - camelCase no disco (combina com o espelho TS).
// - campos opcionais omitidos quando vazios/None para manter os .yml limpos.

use serde::{Deserialize, Serialize};

/// Par chave/valor usado em headers, params, form data, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Se desabilitado, o par existe no arquivo mas nao e enviado na request.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Modo do corpo da request. M1 ja define todos os modos; o payload e carregado
/// conforme o modo (campos None nos demais).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyMode {
    None,
    Json,
    Text,
    Xml,
    FormUrlencoded,
    Multipart,
    Graphql,
}

impl Default for BodyMode {
    fn default() -> Self {
        BodyMode::None
    }
}

/// Payload do GraphQL (query + variables como string JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlBody {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub variables: String,
}

/// Corpo da request. `mode` decide qual campo de payload e relevante.
/// Campos extensiveis: M2+ pode adicionar mais variantes sem quebrar o formato.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    #[serde(default)]
    pub mode: BodyMode,
    /// Texto cru para os modos json/text/xml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// Pares para form_urlencoded e multipart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql: Option<GraphqlBody>,
}

/// Modo de autenticacao. Extensivel: M2 expande oauth2 e afins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    /// Herda a auth definida na pasta/colecao pai.
    Inherit,
    Basic,
    Bearer,
    Apikey,
    Oauth2,
}

impl Default for AuthMode {
    fn default() -> Self {
        AuthMode::None
    }
}

/// Onde uma API key e injetada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyPlacement {
    Header,
    Query,
}

impl Default for ApiKeyPlacement {
    fn default() -> Self {
        ApiKeyPlacement::Header
    }
}

/// Autenticacao da request. So o bloco do `mode` ativo costuma estar preenchido.
/// Estrutura aberta de proposito (M2 expande oauth2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    #[serde(default)]
    pub mode: AuthMode,
    // basic
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    // bearer
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    // apikey
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<ApiKeyPlacement>,
}

/// Scripts pre/pos request (conteudo JS cru; execucao e do M3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scripts {
    #[serde(default)]
    pub pre: String,
    #[serde(default)]
    pub post: String,
}

/// Uma request HTTP individual (gravada em `<slug>.yml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestItem {
    pub name: String,
    /// Ordem de exibicao dentro da pasta/colecao.
    #[serde(default)]
    pub seq: u32,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(default)]
    pub body: Body,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub scripts: Scripts,
    /// Conteudo cru dos testes (execucao e do M3).
    #[serde(default)]
    pub tests: String,
    /// Documentacao em markdown.
    #[serde(default)]
    pub docs: String,
}

fn default_method() -> String {
    "GET".to_string()
}

/// Um no da arvore da colecao: ou uma pasta ou uma request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TreeItem {
    #[serde(rename = "folder")]
    Folder(Folder),
    #[serde(rename = "request")]
    Request(RequestItem),
}

impl TreeItem {
    /// `seq` do item, para ordenar irmaos.
    pub fn seq(&self) -> u32 {
        match self {
            TreeItem::Folder(f) => f.seq,
            TreeItem::Request(r) => r.seq,
        }
    }

    /// Nome de exibicao do item.
    pub fn name(&self) -> &str {
        match self {
            TreeItem::Folder(f) => &f.name,
            TreeItem::Request(r) => &r.name,
        }
    }
}

/// Uma pasta da colecao (diretorio com `folder.yml`). Contem filhos (`items`).
/// `items` NAO e serializado no `folder.yml` — a arvore vem do filesystem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub name: String,
    #[serde(default)]
    pub seq: u32,
    /// Filhos reconstruidos a partir do disco. Vao para o IPC (front precisa da
    /// arvore), mas NUNCA para o folder.yml — o disco usa FolderMeta, nao Folder.
    #[serde(default)]
    pub items: Vec<TreeItem>,
}

/// Config raiz da colecao (gravada em `collection.yml`). `items` vem do disco.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    /// Arvore reconstruida a partir do disco. Vai para o IPC (front precisa da
    /// arvore), mas NUNCA para o collection.yml — o disco usa CollectionMeta.
    #[serde(default)]
    pub items: Vec<TreeItem>,
    /// Variaveis da colecao. Campo aberto para o M2; YAML livre por enquanto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vars: Option<serde_yaml::Value>,
}

fn default_version() -> String {
    "1".to_string()
}

/// Metadados so do `collection.yml` (sem a arvore), usados ao gravar/parsear o
/// arquivo raiz isoladamente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMeta {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vars: Option<serde_yaml::Value>,
    /// Auth herdavel da colecao (F11). Retrocompativel: ausente => None, e
    /// omitido na serializacao quando None para manter o collection.yml limpo.
    /// Requests/pastas com `mode: inherit` sobem ate aqui.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

/// Metadados so do `folder.yml` (sem os filhos).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMeta {
    pub name: String,
    #[serde(default)]
    pub seq: u32,
    /// Auth herdavel da pasta (F11). Retrocompativel: ausente => None, omitido
    /// na serializacao quando None. Requests com `mode: inherit` sobem ate aqui
    /// (e desta para a colecao, se a pasta tambem nao definir auth concreta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGRESSAO (tela preta): a `Collection` cruza o IPC como JSON; o campo
    /// `items` DEVE estar presente, senao o front recebe `undefined` e a Sidebar
    /// quebra ao iterar a arvore. Antes havia `skip_serializing` aqui, que
    /// removia `items` tambem do IPC (nao so do disco).
    #[test]
    fn collection_inclui_items_no_ipc_json() {
        let col = Collection {
            name: "Minha API".to_string(),
            version: "1".to_string(),
            items: vec![TreeItem::Folder(Folder {
                name: "auth".to_string(),
                seq: 0,
                items: vec![TreeItem::Folder(Folder {
                    name: "interno".to_string(),
                    seq: 0,
                    items: vec![],
                })],
            })],
            vars: None,
        };

        let v = serde_json::to_value(&col).unwrap();
        // O front depende destas chaves.
        assert!(v.get("items").is_some(), "items ausente no JSON do IPC");
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        // Arvore aninhada tambem precisa sobreviver ao IPC.
        assert_eq!(v["items"][0]["type"], "folder");
        assert_eq!(v["items"][0]["items"][0]["name"], "interno");
    }

    /// O disco NAO deve receber a arvore: o `collection.yml` usa CollectionMeta,
    /// que nem tem o campo `items`.
    #[test]
    fn collection_meta_nao_tem_items() {
        let meta = CollectionMeta {
            name: "Minha API".to_string(),
            version: "1".to_string(),
            vars: None,
            auth: None,
        };
        let y = serde_yaml::to_string(&meta).unwrap();
        assert!(!y.contains("items"), "collection.yml nao deve conter items");
    }

    // ---- F9/F11 retrocompat: Meta desserializa COM e SEM campos extras ----
    //
    // Garante que ler um collection.yml/folder.yml de versao anterior (sem novos
    // campos) E de versao futura (com um bloco `auth:` ou outras chaves que o
    // Meta nao conhece) NAO quebra. serde sem `deny_unknown_fields` deve tolerar
    // chaves desconhecidas; campos ausentes caem no default. Se algum agente
    // adicionar `deny_unknown_fields`, estes testes pegam a regressao.

    #[test]
    fn collection_meta_desserializa_sem_auth_e_sem_vars() {
        // YAML minimo de uma colecao antiga.
        let y = "name: Antiga\nversion: \"1\"\n";
        let meta: CollectionMeta = serde_yaml::from_str(y).unwrap();
        assert_eq!(meta.name, "Antiga");
        assert_eq!(meta.version, "1");
        assert!(meta.vars.is_none());
    }

    #[test]
    fn collection_meta_desserializa_com_bloco_auth_desconhecido() {
        // collection.yml "do futuro" com um bloco auth que CollectionMeta nao
        // mapeia: deve ser ignorado, nao causar erro de parse.
        let y = "name: Nova\nversion: \"2\"\nauth:\n  mode: bearer\n  token: abc\n";
        let meta: CollectionMeta =
            serde_yaml::from_str(y).expect("auth desconhecido nao deve quebrar o parse");
        assert_eq!(meta.name, "Nova");
        assert_eq!(meta.version, "2");
    }

    #[test]
    fn collection_meta_version_default_quando_ausente() {
        // Sem version -> default "1" (retrocompat de arquivo muito antigo).
        let meta: CollectionMeta = serde_yaml::from_str("name: SoNome\n").unwrap();
        assert_eq!(meta.version, "1");
    }

    #[test]
    fn folder_meta_desserializa_sem_seq_e_sem_auth() {
        // folder.yml antigo: so o nome.
        let meta: FolderMeta = serde_yaml::from_str("name: pasta\n").unwrap();
        assert_eq!(meta.name, "pasta");
        assert_eq!(meta.seq, 0); // default
    }

    #[test]
    fn folder_meta_desserializa_com_bloco_auth_desconhecido() {
        // folder.yml "do futuro" com auth herdavel: ignorado pelo Meta.
        let y = "name: pasta\nseq: 3\nauth:\n  mode: inherit\n";
        let meta: FolderMeta =
            serde_yaml::from_str(y).expect("auth desconhecido nao deve quebrar o parse");
        assert_eq!(meta.name, "pasta");
        assert_eq!(meta.seq, 3);
    }

    // ---- RequestItem JA carrega auth (M2): round-trip com e sem auth ----

    #[test]
    fn request_item_sem_auth_no_yaml_usa_auth_none_default() {
        // Request antiga sem bloco auth -> Auth::default() (mode None).
        let y = "name: req\nmethod: GET\nurl: http://x\n";
        let req: RequestItem = serde_yaml::from_str(y).unwrap();
        assert_eq!(req.auth.mode, AuthMode::None);
        assert!(req.auth.token.is_none());
    }

    #[test]
    fn request_item_com_auth_bearer_round_trip() {
        let req = RequestItem {
            name: "req".to_string(),
            seq: 0,
            method: "GET".to_string(),
            url: "http://x".to_string(),
            headers: vec![],
            params: vec![],
            body: Body::default(),
            auth: Auth {
                mode: AuthMode::Bearer,
                token: Some("tkn".to_string()),
                ..Auth::default()
            },
            scripts: Scripts::default(),
            tests: String::new(),
            docs: String::new(),
        };
        let y = serde_yaml::to_string(&req).unwrap();
        let de: RequestItem = serde_yaml::from_str(&y).unwrap();
        assert_eq!(de.auth.mode, AuthMode::Bearer);
        assert_eq!(de.auth.token.as_deref(), Some("tkn"));
        assert_eq!(de, req);
    }

    #[test]
    fn auth_mode_serializa_snake_case() {
        // O disco precisa de snake_case nos enums (espelho TS bate).
        let a = Auth {
            mode: AuthMode::Apikey,
            ..Auth::default()
        };
        let y = serde_yaml::to_string(&a).unwrap();
        assert!(y.contains("apikey"), "AuthMode deve serializar snake_case");
    }

    // ---- F11: auth herdavel em CollectionMeta/FolderMeta -----------------

    #[test]
    fn collection_meta_auth_none_omitido_no_yaml() {
        // auth None nao deve aparecer no collection.yml (skip_serializing_if).
        let meta = CollectionMeta {
            name: "C".to_string(),
            version: "1".to_string(),
            vars: None,
            auth: None,
        };
        let y = serde_yaml::to_string(&meta).unwrap();
        assert!(!y.contains("auth"), "auth None nao deve serializar");
    }

    #[test]
    fn collection_meta_auth_concreta_round_trip() {
        let meta = CollectionMeta {
            name: "C".to_string(),
            version: "1".to_string(),
            vars: None,
            auth: Some(Auth {
                mode: AuthMode::Bearer,
                token: Some("tkn".to_string()),
                ..Auth::default()
            }),
        };
        let y = serde_yaml::to_string(&meta).unwrap();
        assert!(y.contains("auth"), "auth concreta deve serializar");
        let de: CollectionMeta = serde_yaml::from_str(&y).unwrap();
        assert_eq!(de.auth.as_ref().unwrap().mode, AuthMode::Bearer);
        assert_eq!(de.auth.unwrap().token.as_deref(), Some("tkn"));
    }

    #[test]
    fn folder_meta_auth_inherit_round_trip() {
        let meta = FolderMeta {
            name: "p".to_string(),
            seq: 2,
            auth: Some(Auth {
                mode: AuthMode::Inherit,
                ..Auth::default()
            }),
        };
        let y = serde_yaml::to_string(&meta).unwrap();
        let de: FolderMeta = serde_yaml::from_str(&y).unwrap();
        assert_eq!(de.auth.unwrap().mode, AuthMode::Inherit);
    }

    #[test]
    fn collection_meta_sem_auth_no_yaml_desserializa_none() {
        // YAML antigo (sem auth) -> campo None por default.
        let y = "name: Antiga\nversion: \"1\"\n";
        let meta: CollectionMeta = serde_yaml::from_str(y).unwrap();
        assert!(meta.auth.is_none());
    }

    #[test]
    fn folder_meta_sem_auth_no_yaml_desserializa_none() {
        let meta: FolderMeta = serde_yaml::from_str("name: p\nseq: 0\n").unwrap();
        assert!(meta.auth.is_none());
    }

    #[test]
    fn apikey_placement_default_header() {
        // Auth apikey sem placement no YAML -> placement fica None (o default de
        // aplicacao e header), mas o enum em si tem Header como Default.
        assert_eq!(ApiKeyPlacement::default(), ApiKeyPlacement::Header);
        let y = "mode: apikey\nkey: X-Key\nvalue: v\n";
        let a: Auth = serde_yaml::from_str(y).unwrap();
        assert_eq!(a.mode, AuthMode::Apikey);
        assert!(a.placement.is_none());
    }
}
