# Fundação técnica

Atualizado em 2026-10-12. **Especificação e checklist futuros; nenhuma tarefa de implementação executada.** Este documento concentra a primeira entrega para evitar criar outro dossiê. As decisões aprovadas estão no [plano SDD](../development/sdd-plan.md).

## Objetivo e escopo

Comprovar a integração mínima de Leptos SSR/hidratação WASM, Axum/Tokio e TursoDB local em um pacote de aplicação Rust. Uma instância por país, um processo proprietário do banco e desenvolvimento em loopback.

A fatia entrega inicialização validada, health check, migration de um dado sintético, persistência após restart e uma interação de navegador que usa uma server function. Não entrega identidade real, plebiscito, propostas, voto secreto ou capacidade de produção.

Não criar dados de cidadãos reais. O país de teste deve estar claramente identificado; Brasil pode ser fixture, sem decidir o primeiro país de produção.

## Pré-requisitos a verificar quando a execução for solicitada

| Item | Verificação necessária |
| --- | --- |
| Rust/Cargo e alvo WASM | Fixar toolchain e comprovar os dois builds |
| Leptos/Axum/Tokio/`turso` | Selecionar versões compatíveis e fixar dependências no lockfile |
| Build Leptos e `just` | Verificar ferramentas reais e documentar bootstrap reproduzível |
| Navegador/WebDriver | Compatibilidade com Fantoccini e uma sessão real |
| TursoDB e armazenamento | Acesso ao motor real e caminhos distintos para persistência local e testes |

A sessão anterior identificou Rust/Chrome e downloads temporários. Revalidar o ambiente; não tratar caminhos temporários como instalação atual. PNPM, NestJS, React e PostgreSQL não são pré-requisitos deste projeto. Nenhuma ferramenta será instalada nesta etapa documental.

## Requisitos e critérios observáveis

| ID | Requisito | Evidência exigida |
| --- | --- | --- |
| FND01 | Um pacote de aplicação com capacidades SSR e hydrate separadas | Servidor e WASM compilam; banco/segredos não fazem parte do navegador |
| FND02 | Inicialização em loopback com configuração validada | URL real e health check com readiness limitada; configuração inválida falha claramente |
| FND03 | Banco local TursoDB com migration versionada | Dado sintético gravado/lido pelo app e ainda presente após restart |
| FND04 | Interação hidratada que usa endpoint de servidor | Navegador altera dado sintético e mostra resultado confirmado; ausência de erros bloqueantes de console/rede |
| FND05 | Isolamento de teste | Testes criam bancos descartáveis; banco persistente do usuário não é apagado/resetado |
| FND06 | Verificação reproduzível | Harness documentado, testes de persistência e um E2E Rust passam em execução real |
| FND07 | Proteção de configuração e artefatos | Git ignora banco, segredos e relatórios privados; configuração de produção rejeita provedor de identidade de teste, se presente |
| FND08 | Base de CI e carga | CI verifica builds/testes reproduzíveis; smoke Goose em release com dados sintéticos registra amostras e invariantes, sem prometer capacidade |

Erros de migration, banco indisponível, porta ocupada, timeout de readiness e falha de hidratação devem produzir falha verificável, sem substituir TursoDB por outro motor ou pular navegador silenciosamente. Readiness não deve expor detalhes privados.

## Contratos futuros dos comandos

Não existe Justfile nem qualquer comando abaixo neste checkout. Implementá-los e registrar o comportamento real durante a fundação.

| Comando futuro | Contrato |
| --- | --- |
| `just dev` | Aplicar migrations de desenvolvimento sem reset destrutivo, iniciar em loopback e informar a URL |
| `just check` | Formatação/lint e compilação dos alvos servidor e WASM |
| `just test` | Regras implementadas e integração com TursoDB real em bancos descartáveis |
| `just e2e` | App/browser isolados, interação hidratada, waits limitadas e cleanup só de processos do teste |
| `just load` | Goose em Rust contra release isolado, relatório e falha quando critérios definidos forem violados |

Verificar explicitamente dependências ausentes. CI deve usar os mesmos comandos, não uma validação menor apresentada como equivalente.

## Checklist de execução futura

### Ambiente e scaffold

- [ ] Verificar ferramentas, versões e compatibilidade de navegador/driver.
- [ ] Criar pacote mínimo Leptos/Axum/Tokio com configuração SSR/hydrate e toolchain/lockfile.
- [ ] Configurar loopback, health check e mensagens de inicialização/falha.
- [ ] Definir `.gitignore` e caminhos para banco, segredos e artefatos.

### Persistência e interação

- [ ] Integrar `turso`, migration e dado sintético mínimo.
- [ ] Testar gravação/leitura, rollback/falhas relevantes e persistência após restart.
- [ ] Criar tela mínima e server function para a interação de fundação.
- [ ] Separar configuração de teste e produção; rejeitar mock em produção se houver provider.

### Validação e evidência

- [ ] Implementar Justfile e CI com os contratos descritos acima.
- [ ] Compilar SSR e WASM e executar testes com TursoDB real.
- [ ] Inspecionar interação hidratada em navegador real, console/rede, teclado e viewport mobile.
- [ ] Executar e registrar E2E Rust reproduzível com Fantoccini/WebDriver.
- [ ] Executar smoke Goose em release e verificar o dado persistido após a carga.

## Estado da evidência

| Verificação | Estado |
| --- | --- |
| Scaffold, toolchain e dependências | Não implementados/não verificados nesta entrega |
| Builds SSR/WASM | Não executados |
| Migration, persistência e restart | Não executados |
| Navegador e E2E | Não executados |
| CI e smoke de carga | Não implementados/não executados |

Quando executados, registrar aqui commit/estado do checkout, versões, comandos, resultados, limitações e caminhos dos artefatos úteis. Não criar um relatório separado para repetir esta checklist. Campos pendentes não são checks aprovados.

## Ordem após a fundação

Identidade de teste e sessão/RBAC; setup e seus bloqueios; propostas/assinaturas/debate; votação/auditoria após resolver o protocolo; aceitação operacional com restauração e carga final. A fundação comprova integração técnica e não encerra os critérios de aceitação do produto.
