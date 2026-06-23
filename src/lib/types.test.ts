// Testes da logica pura de src/lib/types.ts: type guards e helper novaRequest.
import { describe, it, expect } from "vitest";
import {
  isFolder,
  isRequest,
  novaRequest,
  normalizarRequest,
  normalizarCollection,
  type Collection,
  type Folder,
  type RequestItem,
  type TreeItem,
} from "./types";

function folderItem(): { type: "folder" } & Folder {
  return { type: "folder", name: "auth", seq: 0, items: [] };
}

function requestItem(): { type: "request" } & RequestItem {
  return { type: "request", ...novaRequest("req") };
}

describe("isFolder", () => {
  it("retorna true para pasta", () => {
    expect(isFolder(folderItem())).toBe(true);
  });

  it("retorna false para request", () => {
    expect(isFolder(requestItem())).toBe(false);
  });
});

describe("isRequest", () => {
  it("retorna true para request", () => {
    expect(isRequest(requestItem())).toBe(true);
  });

  it("retorna false para pasta", () => {
    expect(isRequest(folderItem())).toBe(false);
  });
});

describe("guards sao mutuamente exclusivos", () => {
  it("exatamente um guard e verdadeiro por item", () => {
    const itens: TreeItem[] = [folderItem(), requestItem()];
    for (const it of itens) {
      expect(isFolder(it)).not.toBe(isRequest(it));
    }
  });
});

describe("novaRequest", () => {
  it("usa os defaults esperados de uma GET vazia", () => {
    const r = novaRequest("Minha Req");
    expect(r.name).toBe("Minha Req");
    expect(r.seq).toBe(0);
    expect(r.method).toBe("GET");
    expect(r.url).toBe("");
    expect(r.headers).toEqual([]);
    expect(r.params).toEqual([]);
    expect(r.body).toEqual({ mode: "none" });
    expect(r.auth).toEqual({ mode: "none" });
    expect(r.scripts).toEqual({ pre: "", post: "" });
    expect(r.tests).toBe("");
    expect(r.docs).toBe("");
  });

  it("seq default e 0 quando omitido", () => {
    expect(novaRequest("x").seq).toBe(0);
  });

  it("respeita o seq passado", () => {
    expect(novaRequest("x", 7).seq).toBe(7);
  });

  it("preserva o nome exatamente (sem slugificar)", () => {
    // novaRequest e so o modelo; a slugificacao e do backend.
    expect(novaRequest("Listar Usuários!").name).toBe("Listar Usuários!");
  });

  it("headers/params sao arrays novos e independentes", () => {
    const a = novaRequest("a");
    const b = novaRequest("b");
    a.headers.push({ name: "H", value: "1", enabled: true });
    // Mutar uma request nao afeta a outra (sem array compartilhado).
    expect(b.headers).toEqual([]);
  });

  it("resultado e uma RequestItem valida segundo isRequest", () => {
    const item: TreeItem = { type: "request", ...novaRequest("x") };
    expect(isRequest(item)).toBe(true);
  });
});

describe("normalizarRequest (regressao: tela preta por campos omitidos no IPC)", () => {
  it("preenche headers/params/body.form quando ausentes (serde os omite)", () => {
    const raw = { name: "r", method: "GET", url: "/x" } as Partial<RequestItem>;
    const r = normalizarRequest(raw);
    expect(Array.isArray(r.headers)).toBe(true);
    expect(Array.isArray(r.params)).toBe(true);
    expect(Array.isArray(r.body.form)).toBe(true);
    expect(r.headers).toEqual([]);
    expect(r.params).toEqual([]);
  });

  it("aplica defaults de method/auth/scripts/body.mode", () => {
    const r = normalizarRequest({ name: "r" });
    expect(r.method).toBe("GET");
    expect(r.auth.mode).toBe("none");
    expect(r.body.mode).toBe("none");
    expect(r.scripts).toEqual({ pre: "", post: "" });
  });

  it("preserva valores presentes", () => {
    const r = normalizarRequest({
      name: "r",
      method: "POST",
      headers: [{ name: "Accept", value: "application/json", enabled: true }],
      auth: { mode: "bearer", token: "t" },
    });
    expect(r.method).toBe("POST");
    expect(r.headers).toHaveLength(1);
    expect(r.auth.mode).toBe("bearer");
    expect(r.auth.token).toBe("t");
  });

  it("tolera null/undefined sem quebrar", () => {
    expect(normalizarRequest(null).headers).toEqual([]);
    expect(normalizarRequest(undefined).params).toEqual([]);
  });
});

describe("normalizarCollection", () => {
  it("normaliza requests da arvore recursivamente (folders e requests)", () => {
    const col = {
      name: "c",
      version: "1",
      items: [
        { type: "request", name: "r1", method: "GET", url: "/a" },
        {
          type: "folder",
          name: "f",
          seq: 0,
          items: [{ type: "request", name: "r2", method: "POST", url: "/b" }],
        },
      ],
    } as unknown as Collection;
    const out = normalizarCollection(col);
    const req = out.items[0] as { type: "request" } & RequestItem;
    expect(req.headers).toEqual([]);
    expect(req.params).toEqual([]);
    const folder = out.items[1] as { type: "folder" } & Folder;
    const inner = folder.items[0] as { type: "request" } & RequestItem;
    expect(inner.headers).toEqual([]);
    expect(inner.body.mode).toBe("none");
  });

  it("tolera items ausente", () => {
    const out = normalizarCollection({ name: "c", version: "1" } as Collection);
    expect(out.items).toEqual([]);
  });
});
