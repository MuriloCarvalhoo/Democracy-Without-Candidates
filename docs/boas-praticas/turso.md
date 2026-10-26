# TursoDB (crate `turso`): boas práticas e riscos para alto volume

**Data:** 2026-10-25 · **Verificado em:** `turso` 0.8.3 (com `turso_core`, `turso_sdk_kit`, `turso_parser` 0.8.3), código-fonte do crate e documentação oficial.

**Legenda:** 📖 fato documentado ou lido no código-fonte · 🔎 inferência, ainda não medida neste repositório · ⚠️ medir antes de adotar.

Caminhos de código-fonte são relativos a `~/.cargo/registry/src/index.crates.io-*/`.

## 1. Como o crate faz I/O

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 `Builder::build` liga `async_io`. Quando um passo precisa de disco, o `poll` do future chama `run_io()` **na mesma thread** | `turso-0.8.3/src/lib.rs:329-346, 436-441`; `turso_sdk_kit-0.8.3/src/rsapi.rs:1686-1692` |
| 📖 O backend padrão para arquivo é `UnixIO`, que chama `pread`, `pwrite`/`pwritev` e `fsync` de forma **bloqueante** no próprio submit | `turso_core-0.8.3/io/unix.rs:81-83, 317-336, 455-470` |
| 📖 `io_uring` vem compilado. Seleção: `Builder::with_io("io_uring")`; também existem `"syscall"`, `"memory"` e `with_io_impl` | `turso-0.8.3/src/lib.rs:272-282` |
| 🔎 Nenhum backend tira o I/O da thread que chama. Uma leitura fora do cache de páginas bloqueia um worker Tokio em `pread`, e todo commit bloqueia em `fsync`. O crate não usa `spawn_blocking` | Leitura do código acima |
| 📖 `Connection` é `Send + Sync`; `Database` é `Clone` (envolve um `Arc`) | `turso-0.8.3/src/connection.rs:71`; `lib.rs:358-361` |
| 📖 Uso concorrente da mesma conexão falha na hora (`concurrent use forbidden`), sem esperar | `turso_sdk_kit-0.8.3/src/lib.rs:86-95` |
| 📖 `connect()` é síncrono e tem custo: clona o schema, lê o cabeçalho e atualiza estatísticas | `turso_core-0.8.3/database.rs:2383, 2494-2507, 2546` |

**Prática:** manter conexões de longa duração em pool (o repositório já faz isso) e tratar cada chamada ao banco como possivelmente bloqueante por micro a milissegundos.

## 2. Modelo de concorrência

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 WAL é suportado; `wal2` e o modo rollback não são | [COMPAT.md](https://github.com/tursodatabase/turso/blob/main/COMPAT.md) |
| 📖 O checkpoint automático roda **dentro do commit** quando o WAL passa de 1000 frames. `wal_autocheckpoint` funciona; `wal_checkpoint` é "Partial" | `turso_core-0.8.3/storage/pager.rs:1250-1258`; `storage/wal.rs:4101, 4845`; COMPAT.md |
| 🔎 O writer que cruza o limite paga o I/O do checkpoint, num worker Tokio | Inferência |
| 📖 `synchronous` aceita `OFF`, `NORMAL` e `FULL` (não `EXTRA`). Cada conexão nasce em `FULL` e sem busy handler | COMPAT.md; `turso_core-0.8.3/database.rs:2591-2594` |
| 📖 Em modo async, `busy_timeout` não dorme: acorda o waker e devolve `Pending` | `turso_core-0.8.3/statement.rs:669-680, 750-769`; `busy.rs:43-47` |
| 🔎 Sob disputa, `busy_timeout` vira um laço de reagendamento que gasta CPU por até o timeout configurado. O writer único atrás de mutex evita a maior parte disso | Inferência |
| 📖 Escritas concorrentes: `PRAGMA journal_mode = 'mvcc'` + `BEGIN CONCURRENT`, conflito otimista por linha; a aplicação precisa repetir em `Busy`/`BusySnapshot`/conflito | `turso-0.8.3/examples/concurrent_writes.rs`; [docs](https://docs.turso.tech/tursodb/concurrent-writes.md) |
| 📖 MVCC está em beta ("limited production use… may still contain bugs"); o COMPAT.md ainda diz "experimental" e a documentação diverge no nome do pragma. Em MVCC não há REINDEX, tabelas virtuais, `WITHOUT ROWID`, `AUTOINCREMENT` | [Turso 0.5](https://turso.tech/blog/turso-0.5.0); `grep "not supported in MVCC"` em `turso_core` |

**Prática:** com um processo dono do banco, um writer serializado é o modelo correto e simples. MVCC só depois de medir que o writer único é o gargalo e de aceitar o risco de beta.

## 3. Statements preparados

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 `Connection::query` e `execute` chamam `prepare(sql)` **a cada chamada**: re-parse e recompilação | `turso-0.8.3/src/connection.rs:124-135` |
| 📖 `prepare_cached(sql)` mantém um `HashMap` por conexão e reaproveita o programa compilado; mudança de schema força novo prepare. O mapa não tem limite nem LRU | `turso_sdk_kit-0.8.3/src/rsapi.rs:1136, 1331-1374` |
| 🔎 ⚠️ `prepare_cached` nos caminhos quentes é o ganho mais barato. SQL montado dinamicamente não deve ir para o cache, ou ele cresce sem limite | Inferência; medir com `just load` |

## 4. Maturidade e limites

| Fato | Fonte |
| --- | --- |
| 📖 Ainda não chegou à 1.0; o README pede backups independentes. Criptografia em repouso e WAL multiprocesso são experimentais | [README](https://github.com/tursodatabase/turso) |
| 📖 `PRAGMA foreign_keys` funciona; `foreign_key_check` e `defer_foreign_keys` não. Sem triggers `INSTEAD OF` e sem `recursive_triggers` | COMPAT.md |
| 📖 `VACUUM INTO` funciona sem flag e só reporta sucesso depois de gravar de forma durável | `turso_core-0.8.3/translate/vacuum.rs:11-39`; `vdbe/vacuum.rs:366-369` |
| 🔎 `VACUUM INTO` mantém um snapshot de leitura aberto durante toda a cópia, com leituras síncronas: atrasa o checkpoint e a truncagem do WAL | Inferência |
| 📖 A [página do SDK Rust](https://docs.turso.tech/connect/rust) é só um guia inicial: não há orientação oficial sobre pool, tamanho de transação ou tempo de posse do writer | Docs oficiais |

## 5. Armadilhas de async

| Fato ou inferência | Fonte |
| --- | --- |
| 📖 Só a API tipada `Transaction` marca transação pendente no drop e faz rollback na próxima utilização. `execute("BEGIN IMMEDIATE")` cru não tem essa proteção | `turso-0.8.3/src/transaction.rs:228-240`; `connection.rs:95-120` |
| 🔎 Se o future for cancelado entre `BEGIN` e `COMMIT` (cliente desconecta, timeout, `select!`), a conexão do writer fica em transação aberta segurando o lock de escrita do WAL até a próxima aquisição | Inferência |
| 📖 Statement dropado no meio do passo roda `reset_best_effort()`; `Connection` core dropada em transação faz rollback e solta os locks | `turso_core-0.8.3/statement.rs:1517-1527`; `connection.rs:579-615` |
| 🔎 Segurar o writer enquanto espera trabalho não relacionado (HTTP, serviço facial, Argon2) serializa todas as escritas atrás dessa latência | Inferência |

## 6. Situação neste repositório

Auditoria de `src/infrastructure/db.rs` e das chamadas a `Db::write()` fora de testes e seeds.

| Item | Situação | Ação sugerida |
| --- | --- | --- |
| Writer único + pool de leitores (`db.rs:22-46`) | ✅ Correto para um processo dono do banco | Manter |
| Rollback de transação pendente na próxima aquisição (`db.rs:86-90`) | ✅ Cobre o cancelamento entre `BEGIN` e `COMMIT` | 🔎 O lock do WAL fica preso até a próxima escrita; avaliar rollback em `Drop` de `WriteTx` |
| Trabalho lento dentro da transação de escrita | ✅ Varredura não achou HTTP, face ou Argon2 entre `write()` e `commit()`; a verificação facial roda depois do commit da reserva (`application/identity/mod.rs:289-296`) | Manter como regra; teste de regressão é desejável |
| `query`/`execute` sem cache de statement | ⚠️ Todo SQL é recompilado a cada chamada | Medir `prepare_cached` nas consultas quentes (sessão, IAM, listagem) |
| Disco bloqueando workers Tokio | ⚠️ Inerente ao crate; hoje cada requisição roda I/O no worker | Medir p99 sob carga com banco maior que o cache; se piorar, mover o writer para thread/runtime dedicado |
| Verificação completa da cadeia de auditoria | ✅ **Corrigido em 2026-10-25:** era um único `SELECT` sobre todo `audit_events` (snapshot longo, worker sem ceder); agora páginas de 256 por `seq`, cada uma um statement curto, com `yield_now` entre elas | Manter |
| `synchronous` padrão `FULL` | 📖 Seguro; um `fsync` por commit | Manter `FULL` para votos. Não trocar por `NORMAL` sem decisão explícita |
| `busy_timeout(5s)` em todas as conexões | 🔎 Útil contra checkpoint; sob disputa vira laço de CPU | Manter; observar CPU em carga concentrada |

## 7. Capacidade nesta máquina

Medição real (`artifacts/load/20261025T185408Z`, commit `3633441`, 28 CPUs, gerador na mesma máquina): 7.842 req/s, cerca de 405 escritas/s, p99 ≤ 4 ms, 0 falhas. Isso não é saturação: foram 20 usuários.

| Volume | Taxa média | Avaliação 🔎 |
| --- | --- | --- |
| 1 milhão de acessos/dia | ~12 req/s | Folga grande |
| 1 milhão de acessos/hora | ~280 req/s | Dentro do medido |
| 1 milhão de **votos** em 1 hora | ~280 escritas/s sustentadas | Perto do que foi medido; o limite real é `fsync` por commit do writer único e precisa de rampa até saturação |

O banco não é má prática para esse uso. O teto é o writer único: mede-se em commits/s, não em req/s. Próximo passo: `just load` com rampa até saturação e votação concentrada, como já previsto no plano SDD.
