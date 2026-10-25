# Estrutura do repositório

Atualizado em 2026-10-25. Arquitetura aprovada e implementada como um pacote Cargo de aplicação. O registro das decisões fica no [plano SDD](../development/sdd-plan.md).

## Estado e organização documental

O checkout contém a aplicação Rust, o motor facial Python, testes, ferramentas de carga, exemplos de operação e a documentação abaixo.

| Documento existente | Responsabilidade |
| --- | --- |
| [README](../../README.md) | Entrada, estado real e navegação |
| [PRD](../prd/prd.md) | Produto, critérios R01–R29, requisitos não funcionais e decisões pendentes |
| [Plano SDD](../development/sdd-plan.md) | Decisões de arquitetura, fluxo e estratégia de validação |
| [Fundação](foundation-setup.md) | Requisitos, comandos, checklist e evidência |
| [Servidor](backend-architecture.md) | Processo, camadas e casos de uso |
| [Interface](frontend-architecture.md) | Leptos, hidratação e experiência web |
| [Banco](database-structure.md) | Modelo conceitual e invariantes de persistência |
| [Identidade e autorização](auth-rbac-flow.md) | Sessão, permissões e critérios AUTH |
| [Wireframes](../wireframes/wireframes.md) | Fluxos e estados de tela |
| [AGENTS.md](../../AGENTS.md) | Instruções para trabalho no repositório |
| [Operação](../../ops/README.md) | Implantação, backup e restauração |
| [Motor facial](../../face-service/README.md) | Serviço Python privado e seus limites |

Atualizar esses documentos em vez de criar relatórios de análise ou repetir decisões em novos arquivos. A primeira especificação e checklist estão no guia de fundação. Novos artefatos de feature só devem surgir quando necessários ao trabalho solicitado.

## Estrutura atual

```text
Cargo.toml / Cargo.lock    # um pacote de aplicação (features ssr e hydrate)
rust-toolchain.toml        # Rust 1.96.0 + wasm32
Justfile                   # interface única de comandos
src/
  domain/                  # regras puras: identidade, propostas, setup, votação, debate, auditoria
  application/             # casos de uso, autorização, transações (servidor)
  infrastructure/          # TursoDB, migrations, config, flags, catálogo, segredos, provedores
  features/                # telas Leptos e server functions por funcionalidade
  server/                  # estado, contexto da requisição, HTTP, startup e CLI de operador
  i18n/pt_br/              # textos por funcionalidade
  app.rs, lib.rs, main.rs  # rotas, hidratação e entrada do binário
migrations/                # SQL versionado com checksum
catalog/br/                # catálogo territorial IBGE versionado
style/, public/            # CSS, manifest, service worker e página offline
tests/                     # E2E Fantoccini (e2e*.rs) e harness em tests/support
face-service/              # motor facial Python privado (exceção aprovada)
tools/load/                # Goose em pacote separado
ops/                       # systemd, Caddy, backup e restauração
artifacts/                 # evidências geradas, ignoradas pelo Git
```

## Dependências e fronteiras

- Domínio: regras de elegibilidade, propostas e setup, sem dependência de componentes web ou banco.
- Aplicação: recebe o contexto autenticado do servidor, autoriza ações e coordena persistência. É a fonte das regras mesmo quando invocada diretamente em testes.
- Infraestrutura: implementa persistência, configuração e integração com identidade. Banco, segredos e bibliotecas de servidor não entram no build WASM.
- Features: componentes Leptos, estado da tela e endpoints que delegam aos casos de uso. A UI não valida definitivamente permissões ou resultados eleitorais.
- Transporte compartilhado: tipos públicos mínimos; evitar serializar entidades internas com dados pessoais, biometria, segredos ou dados de voto privado.

Separar capacidades de SSR e hidratação no build. Regras do servidor não podem depender de dados que o navegador possa substituir, como role, país ou alegação de reautenticação.

## País e jurisdições

Cada país tem implantação, subdomínio, configuração e banco próprios, com um processo de aplicação. A instância não escolhe banco por país informado pelo navegador e não exige um catálogo global para funcionar.

A primeira implantação será no Brasil. O login usa e-mail sem documento nacional. Manter idioma e nomes dos níveis territoriais adaptáveis à configuração do país; critérios de elegibilidade e regras eleitorais precisam de especificação própria, sem presumir que as regras de um país servem para todos.

No Brasil, o modelo prevê jurisdições nacional, estadual e municipal; outros países usam a hierarquia configurada para sua implantação. A elegibilidade e o escopo de acesso precisam considerar essa hierarquia; independência entre países não substitui autorização territorial dentro do país. As decisões territoriais estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes).

## Evolução e entrega

As fatias da fundação, identidade, setup, propostas, debate, votação demonstrativa, auditoria, notificações e operação estão implementadas com dados sintéticos. Votação real e voto secreto exigem protocolo e validação.

Não adicionar crates de domínio, filas, Redis, Kubernetes ou microsserviços sem necessidade demonstrada.

Não criar comandos Git destrutivos como parte do fluxo. Preservar alterações locais e usar `/usr/bin/git`; validação não autoriza commit, push ou publicação automática.
