// Testes da logica pura de autenticacao (F11).

import { describe, it, expect } from "vitest";
import {
  resolverAuthEfetiva,
  aplicarAuth,
  base64Utf8,
  mesclarSemSobrescrever,
} from "./auth";
import type { Auth, KeyValue } from "./types";
import type { VarScopes } from "./envScopes";

// Escopos vazios (nenhuma var) por padrao; alguns testes injetam vars.
function scopes(
  patch: Partial<VarScopes> = {},
): VarScopes {
  return {
    runtime: {},
    env: [],
    collection: [],
    global: [],
    ...patch,
  };
}

function variavel(name: string, value: string, secret = false) {
  return { name, value, enabled: true, secret };
}

// ---- base64Utf8 ----------------------------------------------------------

describe("base64Utf8", () => {
  it("codifica ascii simples", () => {
    expect(base64Utf8("user:pass")).toBe("dXNlcjpwYXNz");
  });

  it("string vazia vira vazia", () => {
    expect(base64Utf8("")).toBe("");
  });

  it("padding de 1 byte usa ==", () => {
    expect(base64Utf8("M")).toBe("TQ==");
  });

  it("padding de 2 bytes usa =", () => {
    expect(base64Utf8("Ma")).toBe("TWE=");
  });

  it("bloco exato de 3 bytes sem padding", () => {
    expect(base64Utf8("Man")).toBe("TWFu");
  });

  it("lida com UTF-8 multibyte (acentos)", () => {
    // "á" = C3 A1 -> base64 "w6E="
    expect(base64Utf8("á")).toBe("w6E=");
  });

  it("lida com emoji (4 bytes)", () => {
    // U+1F600 = F0 9F 98 80 -> "8J+YgA=="
    expect(base64Utf8("\u{1F600}")).toBe("8J+YgA==");
  });

  // ---- Fronteiras exatas de largura UTF-8 (mata mutantes `<` -> `<=` no
  // encoder utf8Bytes e a branch de 3 bytes). Cada codepoint esta NO limiar.

  it("U+007F ainda e 1 byte (0x7F)", () => {
    // 7F -> "fw==" (so muda se a fronteira < virar <=)
    expect(base64Utf8("\u{007F}")).toBe("fw==");
  });

  it("U+0080 ja e 2 bytes (limiar code < 0x80)", () => {
    // C2 80 -> "woA=" ; com `code <= 0x80` viraria 1 byte (resultado diferente)
    expect(base64Utf8("\u{0080}")).toBe("woA=");
  });

  it("U+07FF ainda e 2 bytes", () => {
    // DF BF -> "378="
    expect(base64Utf8("\u{07FF}")).toBe("378=");
  });

  it("U+0800 ja e 3 bytes (limiar code < 0x800)", () => {
    // E0 A0 80 -> "4KCA" ; com `code <= 0x800` viraria 2 bytes
    expect(base64Utf8("\u{0800}")).toBe("4KCA");
  });

  it("U+20AC (euro) e 3 bytes (exercita a branch de 3 bytes)", () => {
    // E2 82 AC -> "4oKs" ; mata `else if (false)` e a falta de cobertura da branch
    expect(base64Utf8("\u{20AC}")).toBe("4oKs");
  });

  it("U+FFFF ainda e 3 bytes", () => {
    // EF BF BF -> "77+/"
    expect(base64Utf8("\u{FFFF}")).toBe("77+/");
  });

  it("U+10000 ja e 4 bytes (limiar code < 0x10000)", () => {
    // F0 90 80 80 -> "8JCAgA==" ; com `code <= 0x10000` continuaria 3 bytes
    expect(base64Utf8("\u{10000}")).toBe("8JCAgA==");
  });
});

// ---- resolverAuthEfetiva (heranca) --------------------------------------

describe("resolverAuthEfetiva", () => {
  it("request concreta vence sem subir", () => {
    const req: Auth = { mode: "bearer", token: "t" };
    const folder: Auth = { mode: "basic", username: "u" };
    const col: Auth = { mode: "apikey", key: "k" };
    expect(resolverAuthEfetiva(req, folder, col)).toBe(req);
  });

  it("request inherit sobe para a pasta", () => {
    const folder: Auth = { mode: "bearer", token: "t" };
    const r = resolverAuthEfetiva({ mode: "inherit" }, folder, undefined);
    expect(r).toBe(folder);
  });

  it("request e pasta inherit sobem para a colecao", () => {
    const col: Auth = { mode: "apikey", key: "k", value: "v" };
    const r = resolverAuthEfetiva(
      { mode: "inherit" },
      { mode: "inherit" },
      col,
    );
    expect(r).toBe(col);
  });

  it("tudo inherit/ausente cai em none", () => {
    const r = resolverAuthEfetiva({ mode: "inherit" }, undefined, undefined);
    expect(r.mode).toBe("none");
  });

  it("request undefined sobe para a pasta", () => {
    const folder: Auth = { mode: "bearer", token: "t" };
    expect(resolverAuthEfetiva(undefined, folder, undefined)).toBe(folder);
  });

  it("mode none concreto corta a heranca (nao sobe)", () => {
    // none explicito na request NAO deve herdar a pasta.
    const folder: Auth = { mode: "bearer", token: "t" };
    const none: Auth = { mode: "none" };
    expect(resolverAuthEfetiva(none, folder, undefined)).toBe(none);
  });

  it("pula pasta inherit e usa colecao concreta", () => {
    const col: Auth = { mode: "bearer", token: "c" };
    const r = resolverAuthEfetiva(
      { mode: "inherit" },
      { mode: "inherit" },
      col,
    );
    expect(r).toBe(col);
  });
});

// ---- aplicarAuth ---------------------------------------------------------

describe("aplicarAuth", () => {
  it("none nao produz nada", () => {
    const r = aplicarAuth({ mode: "none" }, scopes());
    expect(r.headers).toEqual([]);
    expect(r.query).toEqual([]);
  });

  it("inherit (defensivo) nao produz nada", () => {
    const r = aplicarAuth({ mode: "inherit" }, scopes());
    expect(r.headers).toEqual([]);
    expect(r.query).toEqual([]);
  });

  it("basic monta Authorization Basic base64(user:pass)", () => {
    const r = aplicarAuth(
      { mode: "basic", username: "user", password: "pass" },
      scopes(),
    );
    expect(r.headers).toEqual([
      { name: "Authorization", value: "Basic dXNlcjpwYXNz", enabled: true },
    ]);
    expect(r.query).toEqual([]);
  });

  it("basic com user/pass ausentes usa string vazia", () => {
    const r = aplicarAuth({ mode: "basic" }, scopes());
    // base64(":") = "Og=="
    expect(r.headers[0].value).toBe("Basic Og==");
  });

  it("basic interpola variaveis", () => {
    const r = aplicarAuth(
      { mode: "basic", username: "{{u}}", password: "{{p}}" },
      scopes({ env: [variavel("u", "user"), variavel("p", "pass")] }),
    );
    expect(r.headers[0].value).toBe("Basic dXNlcjpwYXNz");
  });

  it("bearer monta Authorization Bearer", () => {
    const r = aplicarAuth({ mode: "bearer", token: "abc123" }, scopes());
    expect(r.headers).toEqual([
      { name: "Authorization", value: "Bearer abc123", enabled: true },
    ]);
  });

  it("bearer interpola o token", () => {
    const r = aplicarAuth(
      { mode: "bearer", token: "{{tok}}" },
      scopes({ global: [variavel("tok", "xyz")] }),
    );
    expect(r.headers[0].value).toBe("Bearer xyz");
  });

  it("apikey default vai pro header", () => {
    const r = aplicarAuth(
      { mode: "apikey", key: "X-API-Key", value: "secret" },
      scopes(),
    );
    expect(r.headers).toEqual([
      { name: "X-API-Key", value: "secret", enabled: true },
    ]);
    expect(r.query).toEqual([]);
  });

  it("apikey placement query vai pra query", () => {
    const r = aplicarAuth(
      { mode: "apikey", key: "api_key", value: "secret", placement: "query" },
      scopes(),
    );
    expect(r.query).toEqual([
      { name: "api_key", value: "secret", enabled: true },
    ]);
    expect(r.headers).toEqual([]);
  });

  it("apikey placement header explicito vai pro header", () => {
    const r = aplicarAuth(
      { mode: "apikey", key: "X-Key", value: "v", placement: "header" },
      scopes(),
    );
    expect(r.headers).toHaveLength(1);
    expect(r.query).toHaveLength(0);
  });

  it("apikey sem key nao injeta nada", () => {
    const r = aplicarAuth({ mode: "apikey", value: "v" }, scopes());
    expect(r.headers).toEqual([]);
    expect(r.query).toEqual([]);
  });

  it("apikey interpola key e value", () => {
    const r = aplicarAuth(
      { mode: "apikey", key: "{{kname}}", value: "{{kval}}", placement: "query" },
      scopes({
        collection: [variavel("kname", "token"), variavel("kval", "v42")],
      }),
    );
    expect(r.query).toEqual([
      { name: "token", value: "v42", enabled: true },
    ]);
  });

  it("oauth2 usa o token guardado como Bearer", () => {
    const r = aplicarAuth({ mode: "oauth2", token: "access-tok" }, scopes());
    expect(r.headers).toEqual([
      { name: "Authorization", value: "Bearer access-tok", enabled: true },
    ]);
  });
});

// ---- mesclarSemSobrescrever ---------------------------------------------

describe("mesclarSemSobrescrever", () => {
  const kv = (name: string, value: string): KeyValue => ({
    name,
    value,
    enabled: true,
  });

  it("adiciona o que nao existe", () => {
    const r = mesclarSemSobrescrever([kv("A", "1")], [kv("B", "2")], true);
    expect(r).toEqual([kv("A", "1"), kv("B", "2")]);
  });

  it("nao sobrescreve header existente (case-insensitive)", () => {
    const r = mesclarSemSobrescrever(
      [kv("Authorization", "manual")],
      [kv("authorization", "doauth")],
      true,
    );
    expect(r).toEqual([kv("Authorization", "manual")]);
  });

  it("query e case-sensitive: nomes diferentes por caixa coexistem", () => {
    const r = mesclarSemSobrescrever(
      [kv("Key", "1")],
      [kv("key", "2")],
      false,
    );
    expect(r).toEqual([kv("Key", "1"), kv("key", "2")]);
  });

  it("query nao sobrescreve nome identico", () => {
    const r = mesclarSemSobrescrever(
      [kv("api_key", "manual")],
      [kv("api_key", "auth")],
      false,
    );
    expect(r).toEqual([kv("api_key", "manual")]);
  });

  it("nao muta as listas de entrada", () => {
    const existentes = [kv("A", "1")];
    const novos = [kv("B", "2")];
    mesclarSemSobrescrever(existentes, novos, true);
    expect(existentes).toEqual([kv("A", "1")]);
    expect(novos).toEqual([kv("B", "2")]);
  });

  it("lista vazia de novos retorna copia dos existentes", () => {
    const r = mesclarSemSobrescrever([kv("A", "1")], [], true);
    expect(r).toEqual([kv("A", "1")]);
  });

  it("detecta nome existente que NAO e o primeiro (mata `.some` -> `.every`)", () => {
    // Com `.every`, o match no 2o elemento nao seria detectado e o par de auth
    // seria adicionado em duplicidade. `presente` precisa ser `.some`.
    const r = mesclarSemSobrescrever(
      [kv("X", "1"), kv("Authorization", "manual")],
      [kv("Authorization", "doauth")],
      true,
    );
    expect(r).toEqual([kv("X", "1"), kv("Authorization", "manual")]);
  });
});
