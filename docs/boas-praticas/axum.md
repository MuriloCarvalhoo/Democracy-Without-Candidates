# Axum: boas práticas para alto volume

**Data:** 2026-10-25 · **Verificado em:** axum 0.8.9, tower 0.5.3, tower-http 0.6.11, hyper 1.12.0, reqwest 0.13.5 (docs.rs e código-fonte do crate).

**Legenda:** 📖 fato documentado ou lido no código-fonte · 🔎 inferência, ainda não medida neste repositório · ⚠️ medir antes de adotar.

Assincronismo e paralelismo do Tokio, que o Axum usa, ficam em [rust-async.md](rust-async.md).

## 1. Estado e middleware

| Prática | Fonte |
| --- | --- |
| 📖 Preferir `State` a `Extension`: é verificado em compilação. `Extension` ausente vira `500` em execução | [axum 0.8.9](https://docs.rs/axum/0.8.9/axum/index.html) |
| 📖 `Router::layer` executa de baixo para cima; `ServiceBuilder`, de cima para baixo | [axum::middleware](https://docs.rs/axum/0.8.9/axum/middleware/index.html) |
| 📖 `from_fn` é a forma simples com async/await; um `tower::Service` escrito à mão tem o menor overhead | idem |
| 📖 Erros de middleware que não são `Infallible` exigem `HandleErrorLayer` | idem |

## 2. Backpressure, limites e timeouts

| Prática | Fonte |
| --- | --- |
| 📖 "axum expects that all services used in your app won't care about backpressure". Middleware sensível a backpressure vai **em volta do app inteiro**; "at the very least you should load shed" | [axum::middleware](https://docs.rs/axum/0.8.9/axum/middleware/index.html) |
| 📖 `ConcurrencyLimitLayer` limita requisições simultâneas; `LoadShedLayer` rejeita quando o serviço interno não está pronto | [tower limit](https://docs.rs/tower/latest/tower/limit/concurrency/struct.ConcurrencyLimitLayer.html), [load_shed](https://docs.rs/tower/latest/tower/load_shed/index.html) |
| 📖 `TimeoutLayer::new` está obsoleto desde tower-http 0.6.7; usar `TimeoutLayer::with_status_code`. `RequestBodyTimeoutLayer` limita a leitura do corpo | [tower-http timeout](https://docs.rs/tower-http/latest/tower_http/timeout/struct.TimeoutLayer.html) |
| 📖 `DefaultBodyLimit` (2 MB) só vale para extractors baseados em `Bytes`; `RequestBodyLimitLayer` vale para todas as requisições | [DefaultBodyLimit](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html) |
| 📖 `CompressionLayer` exige features `compression-*`; o predicado padrão ignora imagens, SSE e corpos < 32 bytes | [tower-http compression](https://docs.rs/tower-http/0.6/tower_http/compression/predicate/struct.DefaultPredicate.html) |
| 🔎 Um timeout cancela o future do handler no próximo `.await`. Toda transação aberta precisa reverter quando dropada | Inferência; ver [turso.md §5](turso.md#5-armadilhas-de-async) |

## 3. Servidor HTTP (`axum::serve`)

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 `serve` "doesn't support any configuration"; para ajustes, usar hyper ou hyper-util | [axum::serve](https://docs.rs/axum/0.8.9/axum/fn.serve.html) |
| 📖 O builder de conexão é `Builder::new(TokioExecutor::new())`, **sem `.timer()`** | `axum-0.8.9/src/serve/mod.rs:391` |
| 📖 `header_read_timeout` do hyper (30 s por padrão) só funciona com timer configurado | [hyper http1 Builder](https://docs.rs/hyper/latest/hyper/server/conn/http1/struct.Builder.html) |
| 🔎 Sem timer, `axum::serve` não tem timeout de leitura de cabeçalho nem de conexão ociosa: proteção contra slowloris precisa vir do proxy reverso ou de um servidor hyper-util com timer | Inferência |
| 📖 `TCP_NODELAY` não é ligado por padrão; `ListenerExt::tap_io` liga por socket aceito | [ListenerExt](https://docs.rs/axum/0.8.9/axum/serve/trait.ListenerExt.html) |
| 📖 Erro de `accept` (ex.: `EMFILE`) faz o servidor dormir 1 s antes de tentar de novo | [axum::serve](https://docs.rs/axum/0.8.9/axum/fn.serve.html) |
| 📖 Graceful shutdown espera as requisições em andamento; o exemplo oficial manda pôr timeout, porque essa espera não tem limite | [exemplo graceful-shutdown](https://github.com/tokio-rs/axum/blob/axum-v0.8.9/examples/graceful-shutdown/src/main.rs) |

## 4. Processo e sistema operacional

| Prática | Fonte |
| --- | --- |
| 📖 systemd: o limite rígido de arquivos abertos é 524288 por padrão; programas sem `select(2)` devem subir o limite flexível até o rígido | [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) |
| 🔎 Com limite baixo, `EMFILE` + o sleep de 1 s do `accept` trava todas as conexões novas | Inferência |
| 📖 ⚠️ LTO pode render 10–20%; `codegen-units = 1` pode ajudar; `panic = "abort"` remove `catch_unwind` | [Perf Book: build](https://nnethercote.github.io/perf-book/build-configuration.html) |
| 🔎 `panic = "abort"` no servidor desligaria o `CatchPanicLayer`: não usar | Inferência |
| 📖 ⚠️ jemalloc/mimalloc: ganhos grandes já observados, mas variam por plataforma | idem |
| 🔎 Proxy reverso (Caddy/nginx) termina TLS e cuida de timeouts de cabeçalho, ociosidade e cliente lento. Atrás dele, `ConnectInfo` vê o IP do proxy; confiar em `X-Forwarded-For` só vindo do proxy | Inferência |

## 5. Clientes HTTP de saída (reqwest)

| Prática | Fonte |
| --- | --- |
| 📖 Criar um `Client` e reutilizá-lo: ele guarda o pool de conexões e já usa `Arc` | [reqwest Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html) |
| 📖 `timeout` e `connect_timeout` não têm padrão (sem limite) | [ClientBuilder](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) |

## 6. Situação neste repositório

| Item | Situação | Ação sugerida |
| --- | --- | --- |
| `AppState` via `Extension` (`server/http.rs:57`) | 🔎 Funciona, mas falha em execução se faltar | Baixa prioridade: migrar para `State` quando tocar no router |
| Limite de concorrência e load shedding | ✅ **Corrigido em 2026-10-25:** `protect()` (`server/http.rs`) com `GlobalConcurrencyLimitLayer` (um limite para todas as rotas; `Router::layer` aplica camadas por rota, então `ConcurrencyLimitLayer` daria um limite por rota) + `LoadShedLayer` + `HandleErrorLayer` → 503 imediato. `DWC_MAX_IN_FLIGHT`, padrão 1024 | Calibrar o limite com `just load` até saturação |
| Timeout de requisição | ✅ **Corrigido:** handler 30 s (503) e corpo 30 s (408); testes `slow_handlers_time_out_with_503` e `requests_beyond_the_limit_are_shed_with_503` | Manter |
| `axum::serve` sem timer | ✅ Mitigado: o Caddy de exemplo (`ops/caddy/Caddyfile`) corta cabeçalho em 10 s, corpo em 30 s e ociosidade em 2 min; o app escuta em loopback | Decisão pendente: exigir Caddy ou trocar por hyper-util com timer (`sdd-plan.md`, dúvida 10) |
| `TCP_NODELAY` | ✅ **Ligado** com `ListenerExt::tap_io` (`server/startup.rs`) | Manter |
| Drenagem de 30 s no shutdown (`server/startup.rs:354-360`) | ✅ Segue o exemplo oficial | Manter |
| `RequestBodyLimitLayer` de 2 MiB global (`server/http.rs:51-53`) | ✅ | Manter |
| `CatchPanicLayer` e `panic` sem `abort` no servidor | ✅ | Manter; não ligar `panic = "abort"` no binário |
| Clientes reqwest com `timeout` e `connect_timeout` (`infrastructure/address.rs:105-107`, `face_service.rs:138-140`) e criados uma vez | ✅ | Manter |
| Sem `[profile.release]` com LTO para o servidor (`Cargo.toml`) | ⚠️ | Medir `lto = "thin"` e `codegen-units = 1` com `just load` antes de adotar |
| Compressão e limite de arquivos | ✅ Caddy faz `encode zstd gzip`; `ops/systemd/dwc.service` com `LimitNOFILE=65536` (o padrão flexível é 1024) | Manter |
| Middleware `server_fn_error_status` lê o corpo só de respostas 500 de server function (`server/http.rs:74-92`) | ✅ Custo limitado a erros, máximo 64 KiB | Manter |

## 7. Avaliação para milhões de acessos nesta máquina

🔎 Axum não é o limite: o smoke medido chegou a 7.842 req/s com p99 ≤ 4 ms (ver [turso.md §7](turso.md#7-capacidade-nesta-máquina)). O que faltava para alto volume era **proteção**, não velocidade: limite de concorrência, load shedding, timeouts e um proxy com timeouts de conexão. Desde 2026-10-25 estão no app e no Caddy de exemplo; falta calibrar `DWC_MAX_IN_FLIGHT` com carga até saturação.
