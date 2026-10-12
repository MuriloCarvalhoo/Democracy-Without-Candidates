# Estrutura do repositório

Atualizado em 2026-10-12. Arquitetura aprovada; estrutura de aplicação ainda não implementada. O registro das decisões fica no [plano SDD](../development/sdd-plan.md).

## Estado e organização documental

O checkout contém README, LICENSE, AGENTS.md e documentos de produto, arquitetura e desenvolvimento. Não há `Cargo.toml`, `src/`, migrations, Justfile, CI ou harness de testes. A presença de um plano ou de um índice CodeGraph não significa que exista aplicação executável.

| Documento existente | Responsabilidade |
| --- | --- |
| `README.md` | Entrada, estado real e navegação |
| `docs/prd/prd.md` | Produto, critérios R01–R26 e requisitos não funcionais |
| `docs/development/sdd-plan.md` | Decisões aprovadas, questões abertas e validação |
| `docs/architecture/foundation-setup.md` | Especificação, tarefas e evidência pendente da fundação |
| `docs/architecture/backend-architecture.md` | Servidor e casos de uso |
| `docs/architecture/frontend-architecture.md` | Leptos, hidratação e experiência web |
| `docs/architecture/database-structure.md` | Modelo conceitual e invariantes de persistência |
| `docs/architecture/auth-rbac-flow.md` | Identidade, sessão e autorização |
| `docs/wireframes/WIREFRAMES (3).md` | Fluxos e estados de tela |
| `AGENTS.md` | Instruções para trabalho no repositório |

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

Dentro de um país existem jurisdições nacional, estadual e municipal. A elegibilidade e o escopo de acesso precisam considerar essa hierarquia; independência entre países não substitui autorização territorial dentro do país. Regras de residência e eleitorado estão abertas no PRD.

## Evolução e entrega

Começar com a fundação mínima. Acrescentar identidade de teste, sessão/RBAC e setup antes do fluxo de leis; depois propostas, assinaturas e debate. Votação real e auditoria exigem resolver protocolo e regras eleitorais. Notificações, estatísticas, PWA e operação crescem conforme seus critérios.

Não adicionar crates de domínio, filas, Redis, Kubernetes ou microsserviços sem necessidade demonstrada. O desenho anterior de PNPM, duas aplicações e banco por schema foi substituído; consultar Git apenas para histórico.

Não criar comandos Git destrutivos como parte do fluxo. Preservar alterações locais e usar `/usr/bin/git`; validação não autoriza commit, push ou publicação automática.
