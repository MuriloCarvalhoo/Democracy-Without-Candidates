# Fundação técnica

Atualizado em 2026-10-25. **Fundação implementada e verificada localmente na branch `feat/implementation`**; CI configurado, ainda sem execução remota registrada. As decisões estão no [plano SDD](../development/sdd-plan.md).

## Objetivo e escopo

Comprovar a integração mínima de Leptos SSR/hidratação WASM, Axum/Tokio e TursoDB local em um pacote de aplicação Rust. Uma instância por país, um processo proprietário do banco e desenvolvimento em loopback.

A fatia entrega inicialização validada, health check, migration de um dado sintético, persistência após restart, interação hidratada via server function, E2E reproduzível, CI e smoke de carga. Não entrega identidade real, plebiscito, propostas, voto secreto ou capacidade de produção.

Brasil é o país inicial. Usar dados sintéticos claramente identificados nos testes, sem dados de cidadãos reais. A configuração do país deve permitir reutilizar a aplicação em outras implantações.

## Pré-requisitos verificados

| Item | Verificação necessária |
| --- | --- |
| Rust/Cargo e alvo WASM | Fixar toolchain e comprovar os dois builds |
| Leptos/Axum/Tokio/`turso` | Selecionar versões compatíveis e fixar dependências no lockfile |
| Build Leptos e `just` | Verificar ferramentas reais e documentar bootstrap reproduzível |
| Navegador/WebDriver | Compatibilidade com Fantoccini e uma sessão real |
| TursoDB e armazenamento | Acesso ao motor real e caminhos distintos para persistência local e testes |

Versões usadas em 2026-10-24/25: rustc 1.96.0, cargo-leptos 0.3.11, just 1.59.0, leptos 0.8.22, leptos_axum 0.8.10, axum 0.8.9, tokio 1.54.1, turso 0.8.3, wasm-bindgen 0.2.129, fantoccini 0.22.1, Goose 0.18.1, Chrome/chromedriver 156.0.8078.47, Python 3.12. `just doctor` falha explicitamente quando falta ferramenta.

## Requisitos e critérios observáveis

As referências abaixo relacionam a fundação ao [PRD](../prd/prd.md#12-critérios-de-aceite); contribuição parcial não encerra o requisito de produto.

| ID | Requisito | Evidência exigida | PRD relacionado |
| --- | --- | --- | --- |
| FND01 | Um pacote de aplicação com capacidades SSR e hydrate separadas | Servidor e WASM compilam; banco/segredos não fazem parte do navegador | R25, RNF12/13/15 |
| FND02 | Inicialização em loopback com configuração validada | URL real e health check com readiness limitada; configuração inválida falha claramente | R25, RNF15 |
| FND03 | Banco local TursoDB com migration versionada | Dado sintético gravado/lido pelo app e ainda presente após restart | R25, RNF13 |
| FND04 | Interação hidratada que usa endpoint de servidor | Navegador altera dado sintético e mostra resultado confirmado; ausência de erros bloqueantes de console/rede | R25 |
| FND05 | Isolamento de teste | Testes criam bancos descartáveis; banco persistente do usuário não é apagado/resetado | R25 |
| FND06 | Verificação reproduzível | Harness documentado, testes de persistência e um E2E Rust passam em execução real | R25, RNF15 |
| FND07 | Proteção de configuração e artefatos | Git ignora banco, segredos e relatórios privados; configuração de produção rejeita provedor de identidade de teste, se presente | R02, RNF10 — contribuição parcial |
| FND08 | Base de CI e carga | CI verifica builds/testes reproduzíveis; smoke Goose em release com dados sintéticos registra amostras e invariantes, sem prometer capacidade | R25, RNF02/15 — smoke não demonstra capacidade |

Erros de migration, banco indisponível, porta ocupada, timeout de readiness e falha de hidratação devem produzir falha verificável, sem substituir TursoDB por outro motor ou pular navegador silenciosamente. Readiness não deve expor detalhes privados.

## Comandos

| Comando | Comportamento implementado |
| --- | --- |
| `just dev` | `cargo leptos watch` com `data/dev.db` persistente, migrations sem reset, loopback 127.0.0.1:3000 |
| `just dev-seed` | Fixtures sintéticas (flags, contas, setup, propostas, debate, votação); recusado em produção |
| `just check` | `cargo fmt --check`, clippy SSR (`--all-targets`) e clippy WASM com `-D warnings` |
| `just test` | Testes Rust com TursoDB real em bancos descartáveis |
| `just e2e` | Build cargo-leptos, cópia do binário para `target/e2e/`, app isolado em porta livre, chromedriver/Chrome headless, esperas limitadas; roda todos os alvos `e2e*` |
| `just face-setup` / `just face-test` | venv do motor facial e pytest com motor fake determinístico |
| `just load USERS DURATION` | Release isolado + Goose; relatório em `artifacts/load/`; falha com >1% de falhas inesperadas, p95 acima dos limites ou invariantes violados |
| `just backup-test` | Snapshot online (SIGUSR1) e offline, restauração isolada, integridade, retenção e rejeição de arquivo adulterado |

CI (`.github/workflows/ci.yml`) usa os mesmos comandos. Erros de configuração, migration alterada, porta ocupada e timeout de readiness encerram com mensagem explícita.

## Checklist

### Ambiente e scaffold

- [x] Verificar ferramentas, versões e compatibilidade de navegador/driver.
- [x] Criar pacote Leptos/Axum/Tokio com features `ssr`/`hydrate`, toolchain e lockfile.
- [x] Configurar loopback, health check (`/healthz`) e mensagens de inicialização/falha.
- [x] Definir `.gitignore` e caminhos para banco, segredos e artefatos.

### Persistência e interação

- [x] Integrar `turso`, migrations versionadas com checksum e dado sintético mínimo.
- [x] Testar gravação/leitura, rollback, migration alterada/desconhecida e persistência após reabrir o banco.
- [x] Criar tela e server function da interação de fundação (contador sintético, oculto em produção).
- [x] Separar configuração de teste e produção; produção rejeita provider de identidade sintético.

### Validação e evidência

- [x] Implementar Justfile e CI com os contratos acima.
- [x] Compilar SSR e WASM e executar testes com TursoDB real.
- [ ] Inspecionar console/rede, teclado e mobile com Chrome DevTools MCP — o MCP não conectou (timeout); feito parcialmente com Chrome headless (console sem erros nas rotas principais, screenshots 390×844) e pelas regressões Fantoccini.
- [x] Executar E2E Rust reproduzível com Fantoccini/WebDriver.
- [x] Executar smoke Goose em release e verificar o banco após a carga.
- [ ] Executar o CI no GitHub — configurado, não executado (sem push).

## Estado da evidência

Última execução local completa no commit `7f914f2` (merge de todas as features), máquina Linux x86_64 com 28 CPUs e 63 GiB, compartilhada com builds.

| Verificação | Resultado |
| --- | --- |
| `just check` | Passou |
| `just test` | 147 testes passaram, 0 falhas, 2 ignorados (contrato com serviço facial em execução; desempenho com 100 mil eventos) |
| `just e2e` | 15 testes em 8 alvos passaram: fundação, identidade, propostas, debate, votação, auditoria, admin e PWA |
| `just face-test` | 57 testes passaram |
| `just load 20 30s` | 56.319 requisições, 0 falhas; 20/20 votos confirmados = 20 cédulas no banco, sem duplicatas; `integrity_check` ok; p95 ≤ 5 ms. Experimento local, não capacidade |
| `just backup-test` | Passou: snapshot online durante escritas e offline restaurados, cadeia de auditoria válida |

Persistência após restart é coberta por testes Rust (sessões, prazos, encerramento de votação) e pelo E2E da fundação. Artefatos de carga ficam em `artifacts/load/<timestamp>/`, ignorados pelo Git.

## Ordem após a fundação

As features posteriores foram implementadas com dados sintéticos (ver [estado no SDD](../development/sdd-plan.md#estado-da-implementação)). A fundação comprova integração técnica e não encerra os critérios de aceitação do produto.
