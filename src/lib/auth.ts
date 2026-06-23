// F11 — Autenticacao (LOGICA PURA, alvo de mutation).
//
// Duas responsabilidades puras:
//   1. resolverAuthEfetiva(request, folderAuth?, collectionAuth?) -> Auth
//      Resolve a heranca: se o `mode` da auth for "inherit", sobe na cadeia
//      request -> pasta -> colecao ate achar uma auth concreta (ou cai em none).
//   2. aplicarAuth(auth, scopes) -> { headers, query }
//      Produz os pares (ja interpolados com `{{var}}` via F10) que o envio deve
//      mesclar na request, conforme o modo:
//        none    -> nada
//        basic   -> Authorization: Basic base64(user:pass)
//        bearer  -> Authorization: Bearer <token>
//        apikey  -> header OU query (campo `placement`/`in`) com key/value
//        oauth2  -> Authorization: Bearer <accessToken> (token JA obtido)
//        inherit -> tratado por resolverAuthEfetiva; se chegar aqui, vira none
//
// SEGURANCA: credenciais (user/pass/token/accessToken/value de apikey) NUNCA sao
// logadas aqui nem retornadas em estruturas de aviso. So entram nos headers/query
// finais destinados ao envio. A interpolacao reusa `resolverTemplate` da F10, que
// ja garante que `faltando` carrega apenas NOMES (jamais valores) — e aqui nem
// expomos `faltando`, so o resultado interpolado.

import type { Auth, AuthMode, KeyValue } from "./types";
import type { VarScopes } from "./envScopes";
import { resolverTemplate } from "./interpolation";

/** Resultado da aplicacao de auth: pares a mesclar em headers e/ou query. */
export interface AuthAplicada {
  headers: KeyValue[];
  query: KeyValue[];
}

/** Auth "none" canonica (sem credenciais). */
const AUTH_NONE: Auth = { mode: "none" };

/**
 * Resolve a auth EFETIVA seguindo a heranca. Se `request.mode` for "inherit",
 * sobe para a auth da pasta; se a da pasta tambem for "inherit" (ou ausente),
 * sobe para a da colecao; se nada concreto for achado, retorna `none`.
 *
 * Uma auth e considerada "concreta" quando existe E seu `mode` NAO e "inherit".
 * `mode: "none"` concreto e respeitado (corta a heranca explicitamente).
 *
 * PURA: nao muta nada; devolve a propria referencia da auth escolhida (ou
 * AUTH_NONE). O chamador trata o resultado como somente-leitura.
 */
export function resolverAuthEfetiva(
  request: Auth | undefined,
  folderAuth?: Auth,
  collectionAuth?: Auth,
): Auth {
  // Cadeia do mais especifico ao mais generico.
  const cadeia: Array<Auth | undefined> = [request, folderAuth, collectionAuth];
  for (const elo of cadeia) {
    if (elo === undefined) continue;
    if (elo.mode === "inherit") continue; // herda do proximo nivel acima
    return elo; // primeira auth concreta vence
  }
  // Nada concreto na cadeia (tudo inherit/ausente) -> sem auth.
  return AUTH_NONE;
}

/**
 * Interpola um texto opcional de campo de auth. Strings vazias/undefined viram
 * "" (sem variavel a resolver). So devolve o valor; descarta `faltando` (o
 * aviso de vars faltando do envio e responsabilidade da F10 sobre a request, e
 * nao queremos vazar nomes de campos de credencial aqui).
 */
function interpolarCampo(
  texto: string | undefined,
  scopes: VarScopes,
): string {
  if (texto === undefined || texto === "") return "";
  return resolverTemplate(texto, scopes).valor;
}

/**
 * Codifica uma string UTF-8 em base64 SEM depender de `btoa` (indisponivel no
 * Node/Vitest e que, alem disso, nao lida com bytes >0xFF). Implementacao pura
 * do alfabeto base64 padrao com padding `=`. LOGICA PURA.
 */
export function base64Utf8(texto: string): string {
  const bytes = utf8Bytes(texto);
  const alfabeto =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i];
    const b1 = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
    const trio = (b0 << 16) | (b1 << 8) | b2;
    const c0 = (trio >> 18) & 0x3f;
    const c1 = (trio >> 12) & 0x3f;
    const c2 = (trio >> 6) & 0x3f;
    const c3 = trio & 0x3f;
    out += alfabeto[c0];
    out += alfabeto[c1];
    // Padding conforme quantos bytes reais existem neste bloco.
    out += i + 1 < bytes.length ? alfabeto[c2] : "=";
    out += i + 2 < bytes.length ? alfabeto[c3] : "=";
  }
  return out;
}

/** Converte uma string em seus bytes UTF-8 (sem TextEncoder, puro). PURA. */
function utf8Bytes(texto: string): number[] {
  const bytes: number[] = [];
  for (const ch of texto) {
    let code = ch.codePointAt(0) as number;
    if (code < 0x80) {
      bytes.push(code);
    } else if (code < 0x800) {
      bytes.push(0xc0 | (code >> 6), 0x80 | (code & 0x3f));
    } else if (code < 0x10000) {
      bytes.push(
        0xe0 | (code >> 12),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    } else {
      bytes.push(
        0xf0 | (code >> 18),
        0x80 | ((code >> 12) & 0x3f),
        0x80 | ((code >> 6) & 0x3f),
        0x80 | (code & 0x3f),
      );
    }
  }
  return bytes;
}

/** Monta um par habilitado pronto pra mesclar (name/value/enabled). */
function par(name: string, value: string): KeyValue {
  return { name, value, enabled: true };
}

/**
 * Produz os headers/query da auth, JA interpolados. PURA.
 *
 * - none / inherit: vazio (inherit nao deveria chegar aqui resolvido, mas e
 *   tratado defensivamente como none).
 * - basic: Authorization: Basic base64(user:pass) — user/pass interpolados.
 * - bearer: Authorization: Bearer <token>.
 * - apikey: header (default) ou query conforme `placement`; usa key/value.
 * - oauth2: Authorization: Bearer <token> (token JA obtido pelo comando Rust;
 *   guardado pelo front no campo `token` da auth oauth2).
 */
export function aplicarAuth(auth: Auth, scopes: VarScopes): AuthAplicada {
  const headers: KeyValue[] = [];
  const query: KeyValue[] = [];
  const modo: AuthMode = auth.mode;

  if (modo === "basic") {
    const user = interpolarCampo(auth.username, scopes);
    const pass = interpolarCampo(auth.password, scopes);
    const cred = base64Utf8(`${user}:${pass}`);
    headers.push(par("Authorization", `Basic ${cred}`));
  } else if (modo === "bearer") {
    const token = interpolarCampo(auth.token, scopes);
    headers.push(par("Authorization", `Bearer ${token}`));
  } else if (modo === "apikey") {
    const key = interpolarCampo(auth.key, scopes);
    const value = interpolarCampo(auth.value, scopes);
    // Sem nome de chave nao da pra injetar nada.
    if (key !== "") {
      const alvo = auth.placement === "query" ? query : headers;
      alvo.push(par(key, value));
    }
  } else if (modo === "oauth2") {
    // O accessToken e guardado no campo `token` pelo front apos "Obter token".
    const token = interpolarCampo(auth.token, scopes);
    headers.push(par("Authorization", `Bearer ${token}`));
  }
  // none / inherit -> nada.

  return { headers, query };
}

/**
 * Mescla os pares de auth numa lista existente SEM sobrescrever entradas que o
 * usuario ja definiu manualmente com o mesmo nome (case-insensitive p/ headers).
 * Auth so ADICIONA o que ainda nao existe. PURA.
 *
 * `caseInsensitive` = true para headers (HTTP e case-insensitive no nome);
 * false para query params (nomes de query sao case-sensitive).
 */
// Generica sobre o shape do par (basta ter `name`): funciona tanto com o
// `KeyValue` do store quanto com o `KeyVal` enxuto de envio (http-types).
export function mesclarSemSobrescrever<T extends { name: string }>(
  existentes: T[],
  novos: T[],
  caseInsensitive: boolean,
): T[] {
  const presente = (nome: string): boolean =>
    existentes.some((e) =>
      caseInsensitive
        ? e.name.toLowerCase() === nome.toLowerCase()
        : e.name === nome,
    );
  const adicionar = novos.filter((n) => !presente(n.name));
  return [...existentes, ...adicionar];
}
