# Estrutura do repositório

Atualizado em 2026-10-12. Arquitetura aprovada; estrutura de aplicação ainda não implementada. O registro das decisões fica no [plano SDD](../development/sdd-plan.md).

## Estado e organização documental

O checkout contém README, LICENSE, AGENTS.md e documentos de produto, arquitetura e desenvolvimento. Não há `Cargo.toml`, `src/`, migrations, Justfile, CI ou harness de testes. A presença de um plano ou de um índice CodeGraph não significa que exista aplicação executável.

| Documento existente | Responsabilidade |
| --- | --- |
| [README](../../README.md) | Entrada, estado real e navegação |
| [PRD](../prd/prd.md) | Produto, critérios R01–R26, requisitos não funcionais e decisões pendentes |
| [Plano SDD](../development/sdd-plan.md) | Decisões de arquitetura, fluxo e estratégia de validação |
| [Fundação](foundation-setup.md) | Requisitos, contratos de comandos, tarefas e evidência da primeira entrega |
| [Servidor](backend-architecture.md) | Processo, camadas e casos de uso |
| [Interface](frontend-architecture.md) | Leptos, hidratação e experiência web |
| [Banco](database-structure.md) | Modelo conceitual e invariantes de persistência |
| [Identidade e autorização](auth-rbac-flow.md) | Sessão, permissões e critérios AUTH |
| [Wireframes](../wireframes/wireframes.md) | Fluxos e estados de tela |
| [AGENTS.md](../../AGENTS.md) | Instruções para trabalho no repositório |

Atualizar esses documentos em vez de criar relatórios de análise ou repetir decisões em novos arquivos. A primeira especificação e checklist estão no guia de fundação. Novos artefatos de feature só devem surgir quando necessários ao trabalho solicitado.

## Estrutura futura mínima

A árvore abaixo é uma proposta de organização, não um inventário de arquivos existentes. Criar módulos conforme as fatias executadas; não gerar todas as features vazias antecipadamente.

```text
Cargo.toml                 # um pacote de aplicação
Cargo.lock
rust-toolchain.toml
Justfile                   # interface fina de comandos
src/
  domain/                  # regras, tipos e transições
  application/             # casos de uso, autorização e transações
  infrastructure/          # TursoDB, configuração e provedores
  features/                # telas e server functions por funcionalidade
  app.rs                   # composição Leptos e rotas
  lib.rs                   # pontos compartilhados por SSR/hydrate
  main.rs                  # inicialização do servidor
migrations/                # mudanças versionadas do banco
style/                     # CSS
public/                    # assets públicos
tests/                    # integração e E2E em Rust
tools/load/               # pacote Goose isolado do build WASM
artifacts/                # evidências geradas e ignoradas pelo Git
```

Arquivos exatos de inicialização serão ajustados ao scaffold validado. Um pacote de carga separado não implica múltiplos serviços de produção.

## Dependências e fronteiras

- Domínio: regras de elegibilidade, propostas e setup, sem dependência de componentes web ou banco.
- Aplicação: recebe o contexto autenticado do servidor, autoriza ações e coordena persistência. É a fonte das regras mesmo quando invocada diretamente em testes.
- Infraestrutura: implementa persistência, configuração e integração com identidade. Banco, segredos e bibliotecas de servidor não entram no build WASM.
- Features: componentes Leptos, estado da tela e endpoints que delegam aos casos de uso. A UI não valida definitivamente permissões ou resultados eleitorais.
- Transporte compartilhado: tipos públicos mínimos; evitar serializar entidades internas com documentos, biometria, segredos ou dados de voto privado.

Separar capacidades de SSR e hidratação no build. Regras do servidor não podem depender de dados que o navegador possa substituir, como role, país ou alegação de reautenticação.

## País e jurisdições

Cada país tem implantação, subdomínio, configuração e banco próprios, com um processo de aplicação. A instância não escolhe banco por país informado pelo navegador e não exige um catálogo global para funcionar.

A primeira implantação será no Brasil. Manter documentos de identidade, idioma e nomes dos níveis territoriais adaptáveis à configuração do país; critérios de elegibilidade e regras eleitorais precisam de especificação própria, sem presumir que as regras de um país servem para todos.

No Brasil, o modelo prevê jurisdições nacional, estadual e municipal; outros países usam a hierarquia configurada para sua implantação. A elegibilidade e o escopo de acesso precisam considerar essa hierarquia; independência entre países não substitui autorização territorial dentro do país. As decisões territoriais estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes).

## Evolução e entrega

Começar com a fundação mínima. Acrescentar identidade de teste, sessão/RBAC e setup antes do fluxo de leis; depois propostas, assinaturas e debate. Votação real e auditoria exigem resolver protocolo e regras eleitorais. Notificações, estatísticas, PWA e operação crescem conforme seus critérios.

Não adicionar crates de domínio, filas, Redis, Kubernetes ou microsserviços sem necessidade demonstrada.

Não criar comandos Git destrutivos como parte do fluxo. Preservar alterações locais e usar `/usr/bin/git`; validação não autoriza commit, push ou publicação automática.
