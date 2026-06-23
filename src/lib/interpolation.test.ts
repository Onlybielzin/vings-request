import { describe, it, expect } from "vitest";
import type { VarScopes, Variable } from "./envScopes";
import type { RequestData } from "./http-types";
import { resolverTemplate, interpolarRequest } from "./interpolation";

function v(
  name: string,
  value: string,
  enabled = true,
  secret = false,
): Variable {
  return { name, value, enabled, secret };
}

function scopes(p: Partial<VarScopes> = {}): VarScopes {
  return {
    runtime: {},
    env: [],
    collection: [],
    global: [],
    ...p,
  };
}

describe("resolverTemplate", () => {
  it("texto vazio devolve vazio sem faltando", () => {
    expect(resolverTemplate("", scopes())).toEqual({ valor: "", faltando: [] });
  });

  it("texto sem tokens passa intacto", () => {
    expect(resolverTemplate("http://x/y", scopes())).toEqual({
      valor: "http://x/y",
      faltando: [],
    });
  });

  it("substitui um token resolvido", () => {
    const s = scopes({ env: [v("base", "http://api")] });
    expect(resolverTemplate("{{base}}/users", s)).toEqual({
      valor: "http://api/users",
      faltando: [],
    });
  });

  it("faz trim de espacos internos no nome", () => {
    const s = scopes({ env: [v("base", "http://api")] });
    expect(resolverTemplate("{{ base }}/x", s).valor).toBe("http://api/x");
  });

  it("substitui multiplas ocorrencias do mesmo token", () => {
    const s = scopes({ collection: [v("h", "host")] });
    expect(resolverTemplate("{{h}}-{{h}}", s).valor).toBe("host-host");
  });

  it("token nao resolvido fica literal e entra em faltando", () => {
    const r = resolverTemplate("a{{x}}b", scopes());
    expect(r.valor).toBe("a{{x}}b");
    expect(r.faltando).toEqual(["x"]);
  });

  it("faltando nao repete nomes", () => {
    const r = resolverTemplate("{{x}}{{x}}{{y}}", scopes());
    expect(r.faltando).toEqual(["x", "y"]);
  });

  it("precedencia runtime > env > collection > global", () => {
    const s = scopes({
      runtime: { k: "R" },
      env: [v("k", "E")],
      collection: [v("k", "C")],
      global: [v("k", "G")],
    });
    expect(resolverTemplate("{{k}}", s).valor).toBe("R");
  });

  it("runtime com string vazia vence (chave presente)", () => {
    const s = scopes({ runtime: { k: "" }, env: [v("k", "E")] });
    expect(resolverTemplate("[{{k}}]", s).valor).toBe("[]");
  });

  it("variavel desabilitada nao conta (cai pro proximo escopo)", () => {
    const s = scopes({
      env: [v("k", "E", false)],
      collection: [v("k", "C")],
    });
    expect(resolverTemplate("{{k}}", s).valor).toBe("C");
  });

  it("variavel desabilitada sem fallback fica faltando", () => {
    const s = scopes({ env: [v("k", "E", false)] });
    const r = resolverTemplate("{{k}}", s);
    expect(r.valor).toBe("{{k}}");
    expect(r.faltando).toEqual(["k"]);
  });

  it("NAO reinterpola recursivamente o valor (uma passada)", () => {
    const s = scopes({
      env: [v("a", "{{b}}"), v("b", "FINAL")],
    });
    const r = resolverTemplate("{{a}}", s);
    expect(r.valor).toBe("{{b}}");
    // O `{{b}}` veio do VALOR, nao do texto original -> nao e reinterpolado nem
    // reportado como faltando (uma unica passada sobre o texto de entrada).
    expect(r.faltando).toEqual([]);
  });

  it("delimitador desbalanceado fica literal", () => {
    expect(resolverTemplate("{{x", scopes()).valor).toBe("{{x");
    expect(resolverTemplate("x}}", scopes()).valor).toBe("x}}");
    expect(resolverTemplate("{{x}", scopes()).valor).toBe("{{x}");
  });

  it("token vazio {{}} fica literal e nao vira faltando", () => {
    const r = resolverTemplate("a{{}}b", scopes());
    expect(r.valor).toBe("a{{}}b");
    expect(r.faltando).toEqual([]);
  });

  it("token so com espacos {{   }} fica literal", () => {
    const r = resolverTemplate("{{   }}", scopes());
    expect(r.valor).toBe("{{   }}");
    expect(r.faltando).toEqual([]);
  });

  it("aceita valor de variavel vazio (resolve para vazio)", () => {
    const s = scopes({ env: [v("k", "")] });
    const r = resolverTemplate("[{{k}}]", s);
    expect(r.valor).toBe("[]");
    expect(r.faltando).toEqual([]);
  });

  it("texto literal puro nao gera faltando e e identico", () => {
    // Mata mutante que troque o retorno do replace ou ignore o texto sem token.
    const r = resolverTemplate("so texto sem chaves", scopes());
    expect(r).toEqual({ valor: "so texto sem chaves", faltando: [] });
  });

  it("mistura de resolvidos e faltando: so faltando entra na lista", () => {
    const s = scopes({ env: [v("ok", "OK")] });
    const r = resolverTemplate("{{ok}}-{{no}}-{{ok}}", s);
    expect(r.valor).toBe("OK-{{no}}-{{ok}}".replace("{{ok}}", "OK"));
    // resolvido substitui ambas ocorrencias; faltando guarda so o nao resolvido.
    expect(r.valor).toBe("OK-{{no}}-OK");
    expect(r.faltando).toEqual(["no"]);
  });

  it("ordem de faltando segue a 1a aparicao no texto", () => {
    const r = resolverTemplate("{{z}}{{a}}{{m}}{{a}}", scopes());
    expect(r.faltando).toEqual(["z", "a", "m"]);
  });

  it("token adjacente sem separador resolve os dois", () => {
    const s = scopes({ env: [v("a", "A"), v("b", "B")] });
    expect(resolverTemplate("{{a}}{{b}}", s).valor).toBe("AB");
  });

  it("nome com underscore/numeros resolve (trim nao remove internos)", () => {
    const s = scopes({ env: [v("a_1", "X")] });
    expect(resolverTemplate("{{ a_1 }}", s).valor).toBe("X");
    expect(resolverTemplate("{{a_1}}", s).faltando).toEqual([]);
  });

  it("valor resolvido contendo }} nao quebra (entra cru, uma passada)", () => {
    const s = scopes({ env: [v("a", "fim}}resto")] });
    const r = resolverTemplate("{{a}}", s);
    expect(r.valor).toBe("fim}}resto");
    expect(r.faltando).toEqual([]);
  });
});

function req(p: Partial<RequestData> = {}): RequestData {
  return {
    method: "GET",
    url: "",
    headers: [],
    params: [],
    body: { mode: "none", form: [] },
    ...p,
  };
}

describe("interpolarRequest", () => {
  it("interpola url, headers, params e body raw/form", () => {
    const s = scopes({
      env: [
        v("base", "http://api"),
        v("tok", "abc"),
        v("uid", "42"),
        v("name", "neo"),
      ],
    });
    const original = req({
      url: "{{base}}/users",
      headers: [{ name: "Authorization", value: "Bearer {{tok}}", enabled: true }],
      params: [{ name: "id", value: "{{uid}}", enabled: false }],
      body: {
        mode: "json",
        raw: '{"u":"{{name}}"}',
        form: [{ name: "{{name}}", value: "{{uid}}", enabled: true }],
      },
    });
    const { req: out, faltando } = interpolarRequest(original, s);
    expect(out.url).toBe("http://api/users");
    expect(out.headers[0].value).toBe("Bearer abc");
    expect(out.params[0]).toEqual({ name: "id", value: "42", enabled: false });
    expect(out.body.raw).toBe('{"u":"neo"}');
    expect(out.body.form[0]).toEqual({ name: "neo", value: "42", enabled: true });
    expect(faltando).toEqual([]);
  });

  it("preserva method e timeoutMs sem interpolar", () => {
    const original = req({ method: "POST", timeoutMs: 5000, url: "x" });
    const { req: out } = interpolarRequest(original, scopes());
    expect(out.method).toBe("POST");
    expect(out.timeoutMs).toBe(5000);
  });

  it("nao muta a request de entrada", () => {
    const s = scopes({ env: [v("base", "http://api")] });
    const original = req({ url: "{{base}}/x" });
    interpolarRequest(original, s);
    expect(original.url).toBe("{{base}}/x");
  });

  it("agrega faltando de todos os campos sem repetir", () => {
    const original = req({
      url: "{{a}}",
      headers: [{ name: "{{a}}", value: "{{b}}", enabled: true }],
      params: [{ name: "{{c}}", value: "{{a}}", enabled: true }],
      body: { mode: "json", raw: "{{b}}", form: [] },
    });
    const { faltando } = interpolarRequest(original, scopes());
    expect(faltando).toEqual(["a", "b", "c"]);
  });

  it("body raw undefined continua undefined", () => {
    const original = req({ body: { mode: "none", raw: undefined, form: [] } });
    const { req: out } = interpolarRequest(original, scopes());
    expect(out.body.raw).toBeUndefined();
  });

  it("interpola tambem o nome dos headers/params", () => {
    const s = scopes({ env: [v("hn", "X-Custom")] });
    const original = req({
      headers: [{ name: "{{hn}}", value: "v", enabled: true }],
    });
    const { req: out } = interpolarRequest(original, s);
    expect(out.headers[0].name).toBe("X-Custom");
  });

  it("preserva o enabled de cada par (header/param/form) ao interpolar", () => {
    const s = scopes({ env: [v("x", "X")] });
    const original = req({
      headers: [{ name: "{{x}}", value: "{{x}}", enabled: false }],
      params: [{ name: "p", value: "{{x}}", enabled: true }],
      body: { mode: "json", raw: "", form: [{ name: "f", value: "{{x}}", enabled: false }] },
    });
    const { req: out } = interpolarRequest(original, s);
    expect(out.headers[0].enabled).toBe(false);
    expect(out.params[0].enabled).toBe(true);
    expect(out.body.form[0].enabled).toBe(false);
  });

  it("preserva o mode do body ao interpolar", () => {
    const original = req({ body: { mode: "xml", raw: "<a/>", form: [] } });
    const { req: out } = interpolarRequest(original, scopes());
    expect(out.body.mode).toBe("xml");
  });

  it("ordem de faltando: url, headers, params, body", () => {
    // Mata mutantes que reordenem as chamadas de interpolacao dos campos.
    const original = req({
      url: "{{u}}",
      headers: [{ name: "{{h}}", value: "x", enabled: true }],
      params: [{ name: "{{p}}", value: "x", enabled: true }],
      body: { mode: "json", raw: "{{b}}", form: [] },
    });
    const { faltando } = interpolarRequest(original, scopes());
    expect(faltando).toEqual(["u", "h", "p", "b"]);
  });

  it("body form value entra antes de body raw quando ambos faltam? raw primeiro", () => {
    // raw e interpolado antes do form dentro de interpolarBody.
    const original = req({
      body: { mode: "json", raw: "{{r}}", form: [{ name: "n", value: "{{f}}", enabled: true }] },
    });
    const { faltando } = interpolarRequest(original, scopes());
    expect(faltando).toEqual(["r", "f"]);
  });

  it("name de um par e interpolado antes do value", () => {
    const original = req({
      headers: [{ name: "{{n}}", value: "{{val}}", enabled: true }],
    });
    const { faltando } = interpolarRequest(original, scopes());
    expect(faltando).toEqual(["n", "val"]);
  });

  it("nao muta arrays internos (headers/params/form) da entrada", () => {
    const s = scopes({ env: [v("x", "X")] });
    const headers = [{ name: "h", value: "{{x}}", enabled: true }];
    const form = [{ name: "f", value: "{{x}}", enabled: true }];
    const original = req({ headers, body: { mode: "json", raw: "{{x}}", form } });
    interpolarRequest(original, s);
    expect(original.headers[0].value).toBe("{{x}}");
    expect(original.body.form[0].value).toBe("{{x}}");
    expect(original.body.raw).toBe("{{x}}");
  });

  it("request sem nenhum token devolve faltando vazio e textos intactos", () => {
    const original = req({
      url: "http://x",
      headers: [{ name: "H", value: "v", enabled: true }],
      body: { mode: "json", raw: "{}", form: [] },
    });
    const { req: out, faltando } = interpolarRequest(original, scopes());
    expect(faltando).toEqual([]);
    expect(out.url).toBe("http://x");
    expect(out.headers[0]).toEqual({ name: "H", value: "v", enabled: true });
    expect(out.body.raw).toBe("{}");
  });
});
