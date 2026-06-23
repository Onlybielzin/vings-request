# ruan

Cliente HTTP / API **file-based**, feito em **Tauri v2 + Rust + React**, **otimizado pro Linux**.
Cada coleção é uma pasta no disco e cada request é um arquivo `.yml` — versionável no git, sem
banco de dados, sem nuvem obrigatória.

> Status: **MVP funcional (Milestone 1)**. Já abre coleções, monta e dispara requests e mostra
> a resposta. Variáveis/ambientes, auth, scripting e import/export estão no roadmap abaixo.

## Por que Tauri (e não Electron)

- Binário pequeno (sem Chromium embutido) e baixo uso de RAM — empacota como **`.AppImage`** e **`.deb`**.
- Backend **Rust** faz o HTTP nativo (via `reqwest`): sem CORS, rápido, com timeout/redirects/gzip.
- Perfil de release enxuto (LTO, `opt-level=s`, `strip`, `panic=abort`).

## Stack

| Camada | Tecnologia |
|---|---|
| Shell desktop | Tauri v2 |
| Backend | Rust (`reqwest`, `serde`, `serde_yaml`, `notify`, `tokio`) |
| Frontend | React 19 + TypeScript + Vite |
| Estado | Zustand |
| Editores de código | CodeMirror 6 |
| Testes | Vitest + Stryker (mutation) no front; `cargo test` + `cargo-mutants` no backend |

## Arquitetura

- **Persistência file-based:** coleção = pasta com `collection.yml`; pasta = diretório com
  `folder.yml`; request = `<slug>.yml`. A árvore é reconstruída do filesystem; um watcher
  (`notify`) reflete mudanças do disco na UI.
- **Engine HTTP no Rust:** o webview nunca faz a request — manda os dados via IPC pro backend,
  que executa com `reqwest` e devolve `{status, headers, body, tempo, tamanho}`.
- **Lógica pura isolada:** parsing, interpolação, conversão URL↔params, mapeamento de
  content-type, etc. ficam em módulos puros (`src/lib/*` no front, módulos `store`/`http` no
  Rust) — justamente os alvos do mutation testing.
- **Segurança:** nomes de arquivo passam por slug + validação anti path-traversal e
  canonicalização (nada escreve fora da coleção); leitura de `.yml` tem limite de tamanho;
  inputs de IPC são tratados como não-confiáveis.

## Features

### Milestone 1 — MVP (pronto)

- [x] **F1 — Modelo de dados & formato em disco** (file-based, YAML, parse/stringify isolado, watcher)
- [x] **F2 — Gerenciar coleções** (criar / abrir / fechar; lista persistida entre sessões)
- [x] **F3 — Árvore de pastas e requests** (sidebar com CRUD, menu de contexto, drag-and-drop, badge de método)
- [x] **F4 — Request builder + envio** (método + URL + Send; engine HTTP no Rust com timeout e redirects)
- [x] **F5 — Editor de query params** (tabela ↔ URL, bidirecional, com enable/disable por linha)
- [x] **F6 — Editor de headers** (tabela com enable/disable e autocomplete de headers comuns)
- [x] **F7 — Editor de body** (none / json / text / xml / form-urlencoded / multipart / graphql, com CodeMirror e pretty-print)
- [x] **F8 — Viewer de resposta** (status, tempo, tamanho; abas Body/Headers/Cookies; highlight; busca; preview de imagem/PDF)

### Milestone 2 — Variáveis, ambientes e auth (roadmap)

- [ ] **F9 — Environments & variáveis** (por coleção/global, com variáveis secret)
- [ ] **F10 — Interpolação `{{var}}`** (URL, headers, params, body, auth, com precedência de escopo)
- [ ] **F11 — Autenticação** (Basic, Bearer, API Key, OAuth2; herança de pasta/coleção)

### Milestone 3 — Scripting, testes e produtividade (roadmap)

- [ ] **F12 — Scripts pre-request e post-response** (JS sandbox; `bru.setVar/getVar`, acesso a `req`/`res`)
- [ ] **F13 — Testes / assertions** (painel pass/fail por request)
- [ ] **F14 — Cookie jar** (Set-Cookie por domínio, reenvio automático)
- [ ] **F15 — Tabs / multi-request** (abas com indicador de não-salvo, atalhos)
- [ ] **F16 — Histórico de execuções** (lista cronológica com replay)

### Milestone 4 — Interoperabilidade e extras (roadmap)

- [ ] **F17 — Import / Export** (Postman, OpenAPI, cURL)
- [ ] **F18 — Code generation** (cURL, fetch, axios, Python requests)
- [ ] **F19 — Busca global & command palette** (Ctrl+K)
- [ ] **F20 — Settings** (proxy, SSL verify, timeout, tema; overrides por request)

## Rodando

Pré-requisitos: Node 20+, pnpm, Rust estável, e as libs de sistema do Tauri v2
(`webkit2gtk-4.1`, `libsoup-3.0` no Linux).

```bash
pnpm install
pnpm tauri dev        # sobe o app em modo desenvolvimento
pnpm tauri build      # gera .AppImage e .deb
```

## Testes

```bash
pnpm test             # Vitest (frontend)
pnpm test:mutation    # Stryker (mutation testing do front)
cd src-tauri
cargo test            # testes do backend Rust
cargo mutants         # mutation testing do backend (precisa de cargo-mutants)
```

## Layout de uma coleção em disco

```
minha-api/
  collection.yml          # { name, version }
  listar-usuarios.yml     # request (method, url, headers, params, body, auth, scripts, ...)
  auth/
    folder.yml            # { name, seq }
    login.yml             # request
```

---

Projeto desenvolvido com um pipeline multi-agente (implementação -> revisão de segurança ->
testes automatizados + mutation) por feature.
