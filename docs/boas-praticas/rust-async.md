# Rust assíncrono e paralelismo (Tokio)

**Data:** 2026-10-25 · **Verificado em:** tokio 1.54.1, tokio-util 0.7.20 (docs.rs), Tokio tutorial/blog e blog de Alice Ryhl (mantenedora do Tokio).

**Legenda:** 📖 fato documentado · 🔎 inferência, ainda não medida neste repositório · ⚠️ medir antes de adotar.

Complementa [rust-praticas-da-documentacao.md](../rust-praticas-da-documentacao.md) (memória e velocidade). Específico de cada framework: [axum.md](axum.md), [leptos.md](leptos.md), [turso.md](turso.md).

## 1. O que é bloquear

| Prática | Fonte |
| --- | --- |
| 📖 Código async não deve passar mais de **10 a 100 µs** sem chegar a um `.await` | [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/) |
| 📖 `spawn_blocking` serve para I/O bloqueante; é ruim para CPU cara. CPU pesada: rayon. Trabalho que roda para sempre: thread dedicada | idem |
| 📖 O pool de blocking tem até 512 threads; acima disso, tarefas entram em fila. Tarefas de `spawn_blocking` não podem ser abortadas, e o shutdown espera todas. A doc sugere semáforo para limitar CPU | [spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) |
| 📖 `block_in_place` entra em pânico no runtime `current_thread` e suspende o resto da mesma task; preferir `spawn_blocking` | [block_in_place](https://docs.rs/tokio/latest/tokio/task/fn.block_in_place.html) |

## 2. Mutex em código async

| Prática | Fonte |
| --- | --- |
| 📖 `std::sync::Mutex` é aceitável e muitas vezes preferido quando protege só dados e não atravessa `.await` | [tokio::sync::Mutex](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html) |
| 📖 `tokio::sync::Mutex` existe para ficar travado **através** de `.await`; é FIFO; cancelar `lock()` perde o lugar na fila | idem |
| 📖 Sob disputa forte: uma task dedicada dona do estado, com troca de mensagens, ou sharding | [Shared state](https://tokio.rs/tokio/tutorial/shared-state) |

## 3. Concorrência e paralelismo no runtime

| Prática | Fonte |
| --- | --- |
| 📖 `worker_threads` = número de núcleos por padrão. Cada worker tem fila local de 256 tasks; workers ociosos roubam tarefas. A justiça só vale se nenhuma task bloquear a thread | [tokio::runtime](https://docs.rs/tokio/latest/tokio/runtime/index.html) |
| 📖 Orçamento cooperativo: 128 operações por tick em recursos Tokio; reduziu latência de cauda em até ~3× | [Tokio preemption](https://tokio.rs/blog/2020-04-preemption) |
| 🔎 O orçamento vale só para recursos do Tokio. Um future de outra biblioteca que sempre devolve `Ready` (ex.: linhas já em cache) não cede | Inferência |
| 📖 `JoinHandle` dropado: a task continua, e o resultado ou o pânico se perde | [JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html) |
| 📖 `JoinSet` aborta tudo ao ser dropado; `TaskTracker` + `CancellationToken` é o padrão do guia de shutdown | [JoinSet](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html), [Graceful shutdown](https://tokio.rs/tokio/topics/shutdown) |
| 📖 `select!`: cancel-safe significa que dropar e recriar o future não muda nada. `Mutex::lock`, `Semaphore::acquire`, `read_exact`, `write_all` **não** são cancel-safe | [select!](https://docs.rs/tokio/latest/tokio/macro.select.html) |
| 📖 `interval` usa `MissedTickBehavior::Burst` por padrão: depois de atraso, dispara em rajada; `Delay` e `Skip` evitam isso | [MissedTickBehavior](https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html) |
| 📖 Filas sem limite acabam consumindo toda a memória; limitar canais e sockets abertos | [Channels](https://tokio.rs/tokio/tutorial/channels) |

## 4. Situação neste repositório

| Item | Situação | Ação sugerida |
| --- | --- | --- |
| Argon2 em `spawn_blocking` limitado por semáforo de 1 permissão por CPU (`infrastructure/password.rs:17-33`) | ✅ Segue a doc | Manter |
| Writer em `tokio::sync::Mutex` atravessando `.await` (`infrastructure/db.rs:22-26, 84-91`) | ✅ Uso correto do mutex async; FIFO | 🔎 Latência de escrita ≈ fila × duração da transação: manter transações curtas |
| Pool de leitores em `std::sync::Mutex` (`infrastructure/db.rs:25, 50`) | ✅ Seção crítica curta, sem `.await` | Manter |
| I/O de disco do Turso dentro dos workers | ⚠️ Ver [turso.md §1](turso.md#1-como-o-crate-faz-io) | Medir p99 com banco maior que a RAM de cache |
| Verificação completa da auditoria | ✅ **Corrigido em 2026-10-25:** antes não cedia a thread nenhuma vez (teste `full_walk_yields_to_other_tasks` mediu 0); agora páginas de 256 eventos com `yield_now` entre elas (`application/audit.rs`). 100 mil eventos: 530 ms em release, ~1,3 ms por página | Manter |
| Tarefas de fundo | ✅ **Corrigido:** `server/background.rs` supervisiona lembretes, prazos, verificador e snapshot: pânico é registrado e a tarefa reinicia em 5 s; param depois da drenagem do servidor (`JoinSet` + abort da tarefa supervisionada) | Manter |
| `MissedTickBehavior` | ✅ **Corrigido:** `Delay` em lembretes e verificador, como `phases::run_ticker` | Manter |
| Log de acesso por requisição em `stderr`, síncrono (`server/observability.rs:62-69, 97-110`) | ⚠️ Cada linha trava `stderr` num worker; com journald lento, a escrita bloqueia a requisição | Medir; se pesar, `tracing-appender::non_blocking` com descarte quando a fila enche |
| `tokio::spawn` por requisição | ✅ Nenhum no caminho da requisição | Manter |

## 5. Regra prática

1. Nunca bloquear um worker por mais de ~100 µs sem `.await`.
2. CPU pesada: `spawn_blocking` com semáforo, ou rayon.
3. Toda fila, canal, pool e concorrência tem limite.
4. Toda task de fundo tem dono: tracker, cancelamento e registro de pânico.
5. Todo `.await` é ponto de cancelamento: estado precisa continuar consistente se o future for dropado ali.
