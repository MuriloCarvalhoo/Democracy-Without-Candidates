# Democracy Without Candidates

Projeto de democracia direta para cidadãos proporem, debaterem e votarem decisões coletivas, com regras definidas por plebiscito em cada país.

**Demonstração em desenvolvimento, somente com dados sintéticos.** Não há validade eleitoral: identidade real, unicidade de pessoa e voto secreto ainda não foram validados.

Implantação inicial no Brasil, com configuração por país para reutilizar a aplicação em outros países. Uso exclusivamente online, sem votação ou coleta de assinaturas em meio físico.

## Funcionalidades

- Cadastro com e-mail, senha, nome e endereço por CEP; login com senha, rosto e TOTP.
- Plebiscito de configuração inicial, propostas, assinaturas, debate e emendas.
- Votação pública demonstrativa com resultado auditável; voto privado bloqueado até existir protocolo validado.
- Auditoria pública, notificações, painel do administrador da instância e estatísticas agregadas.

## Uso local

Requisitos: Rust (versão fixada em `rust-toolchain.toml`), `cargo-leptos` 0.3.11, `just`, Chrome e chromedriver compatíveis para os testes de navegador, Python 3.11+ para o motor facial.

```sh
just doctor     # verifica as ferramentas
just dev-seed   # contas, setup e propostas sintéticas em data/dev.db
just dev        # http://127.0.0.1:3000
```

As credenciais sintéticas de desenvolvimento são impressas por `just dev-seed`. Verificação: `just check`, `just test`, `just e2e`, `just face-test`, `just load` e `just backup-test`.

## Documentação

- [Requisitos do produto](docs/prd/prd.md)
- [Decisões e plano de desenvolvimento](docs/development/sdd-plan.md)
- [Arquitetura e organização](docs/architecture/repository_structure.md)
- [Fundação e evidências](docs/architecture/foundation-setup.md)
- [Operação e implantação](ops/README.md)

## Contribuição e dúvidas

Sugestões e dúvidas podem ser discutidas nas issues. Correções são bem-vindas por pull request.

## Licença

[MIT](LICENSE).

## Senha para teste Local 

  ┌────────────────┬────────────────────────────────┐
  │     Campo      │             Valor              │
  ├────────────────┼────────────────────────────────┤
  │ E-mail         │ master@dev.invalid             │
  ├────────────────┼────────────────────────────────┤                                                                                                
  │ Senha          │ senha-sintetica-dev            │                                                                                                            
  ├────────────────┼────────────────────────────────┤
  │ Frase de rosto │ rosto-sintetico-dev            │
  ├────────────────┼────────────────────────────────┤
  │ TOTP           │ pulado com DWC_DEV_SKIP_TOTP=1 │
  └────────────────┴────────────────────────────────┘

