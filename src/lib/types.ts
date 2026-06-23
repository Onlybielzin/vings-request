// Espelho TS do schema Rust (src-tauri/src/store/models.rs).
// Mantenha sincronizado: serde usa camelCase no disco e no IPC, entao os campos
// aqui batem 1:1 com as structs Rust.

/** Par chave/valor (headers, params, form data). */
export interface KeyValue {
  name: string;
  value: string;
  /** Se false, o par existe no arquivo mas nao e enviado. */
  enabled: boolean;
  description?: string;
}

/** Modo do corpo da request (snake_case, igual ao serde do Rust). */
export type BodyMode =
  | "none"
  | "json"
  | "text"
  | "xml"
  | "form_urlencoded"
  | "multipart"
  | "graphql";

export interface GraphqlBody {
  query: string;
  /** Variables como string JSON. */
  variables: string;
}

export interface Body {
  mode: BodyMode;
  /** Texto cru para json/text/xml. */
  raw?: string;
  /** Pares para form_urlencoded e multipart. */
  form?: KeyValue[];
  graphql?: GraphqlBody;
}

/** Modo de autenticacao (extensivel em M2). */
export type AuthMode =
  | "none"
  | "inherit"
  | "basic"
  | "bearer"
  | "apikey"
  | "oauth2";

export type ApiKeyPlacement = "header" | "query";

export interface Auth {
  mode: AuthMode;
  // basic
  username?: string;
  password?: string;
  // bearer
  token?: string;
  // apikey
  key?: string;
  value?: string;
  placement?: ApiKeyPlacement;
}

export interface Scripts {
  pre: string;
  post: string;
}

/** Uma request HTTP individual (gravada em <slug>.yml). */
export interface RequestItem {
  name: string;
  /** Ordem de exibicao dentro da pasta/colecao. */
  seq: number;
  method: string;
  url: string;
  headers: KeyValue[];
  params: KeyValue[];
  body: Body;
  auth: Auth;
  scripts: Scripts;
  /** Conteudo cru dos testes (execucao e do M3). */
  tests: string;
  /** Documentacao em markdown. */
  docs: string;
}

/** Pasta da colecao (diretorio com folder.yml). */
export interface Folder {
  name: string;
  seq: number;
  items: TreeItem[];
  /**
   * Auth herdavel da pasta (F11). Opcional/retrocompativel: ausente => sem auth
   * de pasta. Espelha `FolderMeta.auth` no Rust. Requests com `mode: 'inherit'`
   * sobem ate aqui (e desta para a colecao).
   */
  auth?: Auth;
}

/**
 * No da arvore: pasta ou request. Discriminado por `type`, igual ao serde
 * `#[serde(tag = "type")]` do Rust (folder | request).
 */
export type TreeItem =
  | ({ type: "folder" } & Folder)
  | ({ type: "request" } & RequestItem);

/** Config raiz da colecao (collection.yml + arvore reconstruida do disco). */
export interface Collection {
  name: string;
  version: string;
  items: TreeItem[];
  /** Variaveis da colecao — campo aberto para o M2. */
  vars?: unknown;
  /**
   * Auth herdavel da colecao (F11). Opcional/retrocompativel. Espelha
   * `CollectionMeta.auth` no Rust. Topo da cadeia de heranca de `mode: 'inherit'`.
   */
  auth?: Auth;
}

/** Type guards para discriminar TreeItem. */
export function isFolder(
  item: TreeItem,
): item is { type: "folder" } & Folder {
  return item.type === "folder";
}

export function isRequest(
  item: TreeItem,
): item is { type: "request" } & RequestItem {
  return item.type === "request";
}

/**
 * Normaliza uma RequestItem vinda do IPC. O serde do backend OMITE campos vazios
 * (`skip_serializing_if = Vec::is_empty` em headers/params/body.form, `Option::is_none`
 * nos demais), entao esses campos chegam `undefined` no front. Esta funcao garante
 * o shape completo para a UI nunca quebrar ao iterar (`undefined.map`) — mesma
 * classe de bug que ja derrubou a arvore por `items` ausente.
 */
export function normalizarRequest(
  raw: Partial<RequestItem> | null | undefined,
): RequestItem {
  const r = raw ?? {};
  return {
    name: r.name ?? "",
    seq: r.seq ?? 0,
    method: r.method ?? "GET",
    url: r.url ?? "",
    headers: r.headers ?? [],
    params: r.params ?? [],
    body: {
      mode: r.body?.mode ?? "none",
      raw: r.body?.raw,
      form: r.body?.form ?? [],
      graphql: r.body?.graphql,
    },
    auth: { mode: "none", ...(r.auth ?? {}) },
    scripts: { pre: r.scripts?.pre ?? "", post: r.scripts?.post ?? "" },
    tests: r.tests ?? "",
    docs: r.docs ?? "",
  };
}

/** Normaliza recursivamente os nos de uma arvore (requests ganham shape completo). */
export function normalizarTreeItem(item: TreeItem): TreeItem {
  if (item.type === "folder") {
    return { ...item, items: (item.items ?? []).map(normalizarTreeItem) };
  }
  return { type: "request", ...normalizarRequest(item) };
}

/** Normaliza uma colecao recem-carregada do IPC (arvore inteira). */
export function normalizarCollection(col: Collection): Collection {
  return { ...col, items: (col.items ?? []).map(normalizarTreeItem) };
}

/** Cria uma RequestItem padrao (GET vazia) com o nome dado. */
export function novaRequest(name: string, seq = 0): RequestItem {
  return {
    name,
    seq,
    method: "GET",
    url: "",
    headers: [],
    params: [],
    body: { mode: "none" },
    auth: { mode: "none" },
    scripts: { pre: "", post: "" },
    tests: "",
    docs: "",
  };
}
