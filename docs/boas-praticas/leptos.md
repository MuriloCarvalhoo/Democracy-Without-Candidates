# Leptos (SSR + hidratação): boas práticas para alto volume

**Data:** 2026-10-25 · **Verificado em:** leptos 0.8.22, leptos_axum 0.8.10, leptos_router 0.8.17, leptos_server 0.8.8, server_fn 0.8.13, reactive_graph 0.2.16 (código-fonte do crate, livro e issues do GitHub). A linha 0.9 está em `0.9.0-rc0`.

**Legenda:** 📖 fato documentado ou lido no código-fonte · 🔎 inferência, ainda não medida neste repositório · ⚠️ medir antes de adotar.

Caminhos `R/` são relativos a `~/.cargo/registry/src/index.crates.io-*/`. Issues: `https://github.com/leptos-rs/leptos/issues/NNNN`.

## 1. Modos de SSR

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 `OutOfOrder` (padrão): shell muito rápido; fragmentos suspensos exigem JS; meta tags mais fracas. `PartiallyBlocked`: resposta inicial mais lenta. `InOrder`: pausa em cada `Suspense`. `Async`: TTFB pior. Em rotas aninhadas vence o modo mais restritivo | `R/leptos_router-0.8.17/src/ssr_mode.rs:28-35` |
| 📖 Status e cabeçalhos são fixados no **primeiro chunk** do stream; mudanças posteriores em `ResponseOptions` se perdem | `R/leptos_integration_utils-0.8.9/src/lib.rs` (`from_app`) |
| 📖 `redirect()` dentro de `Resource` não bloqueante não funciona com streaming; para cabeçalho, o resource precisa ser blocking | `R/leptos_axum-0.8.10/src/lib.rs:212-213`; [#3147](https://github.com/leptos-rs/leptos/issues/3147) |
| 📖 Resources blocking aninhados ainda fazem streaming (bug aberto) | [#3280](https://github.com/leptos-rs/leptos/issues/3280) |

**Prática:** cookies e redirecionamentos em server functions (POST) ou num `Resource::new_blocking` de nível superior, nunca dentro de `Suspense` aninhado.

## 2. Resources e hidratação

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 `Resource` roda no servidor, serializa o valor e o cliente desserializa sem refazer a busca | `R/leptos_server-0.8.8/src/resource.rs:270-278` |
| 📖 IDs de resource seguem a ordem de criação: criar um resource condicionalmente em só um dos lados quebra a hidratação | `resource.rs:297-300` |
| 📖 Resource blocking segura toda a resposta até carregar; `LocalResource` fica sempre pendente no servidor; `OnceResource` carrega uma vez | `resource.rs:282, 339-341`; `local_resource.rs:29-33` |
| 📖 HTML inválido (div dentro de p, `<tbody>` ausente) e renderização dependente de `cfg!` causam erro de hidratação | [Livro: hydration bugs](https://book.leptos.dev/ssr/24_hydration_bugs.html) |
| 🔎 Cascata (waterfall) surge quando um resource só é criado depois que outro resolve. Criar resources irmãos logo no início | Inferência |
| 📖 Islands evitam serializar os dados duas vezes (24 kB contra 274 kB de WASM no exemplo do livro), ao custo do roteamento no cliente | [Livro: islands](https://book.leptos.dev/islands.html) |

## 3. Server functions

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 "Any server function can be accessed by any HTTP client": validar entrada e autorização no corpo | `R/server_fn-0.8.13/src/lib.rs:66-69` |
| 📖 Padrão: entrada `PostUrl`, saída `Json`; há Cbor, MsgPack, Postcard, Rkyv, Bitcode, Multipart e Streaming | `server_fn/src/codec/` |
| 📖 Erro sempre sai como **500**, salvo `ResponseOptions::set_status` | `server_fn/src/response/http.rs:55-57` |
| 📖 `extract()` clona todo o `Parts` (cabeçalhos e extensões) a cada chamada | `R/leptos_axum-0.8.10/src/lib.rs:2019-2033` |
| 📖 No SSR, chamar uma server function a partir do servidor executa o corpo direto: sem HTTP e **sem middleware** da função | `R/server_fn_macro-0.8.12/src/lib.rs:780-788` |

## 4. Concorrência no servidor

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 Cada render SSR tem seu `Owner::new_root`; server functions usam `Owner::new()`. Effects não rodam no servidor | `leptos_axum lib.rs:389`; `R/reactive_graph-0.2.16/src/effect/effect.rs:77` |
| 📖 No servidor, `spawn` = `tokio::spawn` e `spawn_local` = `tokio::task::spawn_local` (pânico fora de `LocalSet`) | `R/any_spawner-0.3.0/src/lib.rs:178-185` |
| 📖 **Bug [#4946](https://github.com/leptos-rs/leptos/issues/4946), corrigido só na linha 0.9 (PR #4956, 2026-10-18):** `Owner::new_root` deixa o owner como atual na thread do worker e nada restaura o anterior. `Owner::new()` se pendura nele. Uma busca de contexto que falha localmente sobe para o owner **de outra requisição**: pode devolver o `ResponseOptions` dela ou travar o worker para sempre (`with_context` segura o read lock enquanto percorre os pais). O relato mede ~1 trava a cada 2.000 carregamentos | Issue #4946; `R/reactive_graph-0.2.16/src/owner/context.rs:46-74` |
| 📖 `file_and_error_handler_with_context`, para todo arquivo estático com status 200, cria `Owner::new()` e chama `use_context::<ResponseOptions>()` | `R/leptos_axum-0.8.10/src/lib.rs:2066-2083` |
| 📖 `generate_route_list` liga um `AtomicBool` global de supressão de resources: chamar só na inicialização | `leptos_router generate_route_list.rs:237`; `resource.rs:37-49` |
| 📖 `StaticRoute` pode misturar corpo de um render com cabeçalhos de outro (bug aberto) | [#4772](https://github.com/leptos-rs/leptos/issues/4772) |

## 5. Produção

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 Perfil WASM: `opt-level = 'z'`, `lto = true`, `codegen-units = 1`, `panic = "abort"`; cargo-leptos roda `wasm-opt -Oz` | [start-axum](https://github.com/leptos-rs/start-axum/blob/main/Cargo.toml) |
| 📖 "Always serve compressed WASM in production" | [Livro: binary size](https://book.leptos.dev/deployment/binary_size.html) |
| 📖 Os arquivos estáticos usam `ServeDir` com `.gz`/`.br` pré-comprimidos só se existirem em disco; o handler **não envia `Cache-Control`** e só repassa `Accept-Encoding` (sem 304) | `R/leptos_axum-0.8.10/src/lib.rs:2152-2181` |
| 📖 Com `hash-files`, `HydrationScripts` lê `hash.txt` com `std::fs` síncrono **a cada render** | `R/leptos-0.8.22/src/hydration/mod.rs:124-150` |
| 📖 Divisão do bundle: `--split`, `#[lazy]`, `#[lazy_route]` | Livro: binary size |

## 6. Situação neste repositório

| Item | Situação | Ação sugerida |
| --- | --- | --- |
| Fallback `file_and_error_handler_with_context(provide_context(AppState))` (`server/http.rs:43-46`) | ❌ **Exposto ao #4946.** Todo `/pkg/*.wasm`, `.js`, `.css` e ícone passa por ele. O `ResponseOptions` não é fornecido localmente, então a busca sobe para o owner da última página renderizada naquele worker. 🔎 Consequências: o arquivo estático pode sair com status ou `Set-Cookie` de outra requisição (ex.: o `Max-Age=0` de `clear_session_cookie` desconectando outro usuário), e um worker pode travar sem erro | Servir `site_root` por `tower_http::services::ServeDir` próprio, antes do fallback, e deixar o fallback só para a 404. Atualizar para a versão corrigida quando sair na 0.8 ou ao migrar para 0.9. Escrever teste que reproduza o vazamento antes da correção |
| Server functions com `Owner::new()` (`leptos_axum lib.rs:389`) | 🔎 Mesmo bug: só é seguro se todo `use_context` dentro delas achar o valor localmente. `AppState`, `Parts` e `ResponseOptions` são locais | Não usar `use_context` de tipos não fornecidos no servidor |
| `/pkg` sem `Cache-Control` | ⚠️ Cada visita baixa de novo WASM e JS; o build de debug tem 19,8 MB de WASM | `Cache-Control: public, max-age=31536000, immutable` só nos arquivos com hash; compressão no Caddy ou `.br` gerado no build; medir o WASM de release |
| `instance_info` no `Layout` chama `current_actor`, que pode chamar `clear_session_cookie` durante o streaming (`features/shell.rs:62-69`, `server/context.rs:46-53`) | 🔎 Com `OutOfOrder`, o cookie de limpeza pode se perder depois do primeiro chunk | Baixo impacto (a sessão já expirou no banco); limpar o cookie em server function ou resource blocking |
| Sessão resolvida várias vezes por página (Layout, badge de notificações e cada resource de página chamam `current_actor`) | 🔎 N leituras de sessão + IAM por render | Memoizar o actor por requisição no contexto; medir |
| `extract()` repetido em `session_token`, `auth_token`, `client_ip` (`server/context.rs`) | 🔎 Cada um clona o `Parts` inteiro | Baixo custo; agrupar se aparecer em perfil |
| `hash.txt` lido a cada render | 🔎 Leitura síncrona pequena (cache do SO) | Observar em perfil; sem ação agora |
| Perfil `wasm-release` (`Cargo.toml`) | ✅ Igual ao start-axum | Manter |
| Server functions com endpoints fixos, autorização no servidor e status mapeado pelo middleware (`server/http.rs:74-92`) | ✅ Segue a documentação | Manter |
| `spawn_local` só em handlers de evento do cliente (`features/*.rs`) | ✅ Nunca roda no SSR | Manter |

## 7. Avaliação para milhões de acessos nesta máquina

🔎 O SSR do Leptos é rápido o bastante (página `/propostas` com p99 de 3 ms no smoke). Os riscos reais para alto volume são o **#4946**, que piora com mais carga e mais workers, e o envio de WASM sem cache. Os dois têm correção barata e devem vir antes de qualquer campanha de carga final.
