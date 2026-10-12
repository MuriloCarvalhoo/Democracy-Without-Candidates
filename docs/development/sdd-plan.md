# Decisões e plano de desenvolvimento SDD

Atualizado em 2026-10-12. **Arquitetura e fluxo aprovados; aplicação não implementada.** O escopo atual é documentação. Executar a fundação, instalar ferramentas ou criar código exige solicitação de implementação.

## Registro de decisões

| Tema | Decisão aprovada | Consequência |
| --- | --- | --- |
| Linguagem | Rust para lógica de aplicação e cenários de teste | A saída web inclui HTML/CSS e JavaScript/WASM gerado |
| Framework web | Leptos fullstack, SSR e hidratação WASM | Server functions como interface inicial |
| Runtime HTTP | Axum integrado ao Leptos, com Tokio | Um processo de aplicação e uma implantação |
| Arquitetura | Monolito modular, inicialmente um pacote Cargo de aplicação | Domínio e aplicação separados de UI e persistência |
| Banco | TursoDB local pela crate Rust `turso` | Um processo proprietário do banco persistente |
| Países | Subdomínio, VPS, banco e configuração independentes por país; início no Brasil | Mesma aplicação configurável para outros países, sem banco global necessário à fundação |
| Uso | Exclusivamente online, sem votação ou coleta de assinaturas em meio físico | PWA instalável não implica funcionamento offline |
| SDD | Especificações Markdown enxutas inspiradas no OpenSpec, sem CLI obrigatório | Uma feature ou mudança por vez, com requisitos, tarefas e evidência |
| Instruções de agentes | Um AGENTS.md curto na raiz | Arquivos locais somente para exceções reais |
| Harness de validação | Justfile fino com comandos locais e de CI | Contratos na [fundação](../architecture/foundation-setup.md#contratos-futuros-dos-comandos); ainda não implementados |
| Navegador | Chrome DevTools MCP para inspeção; Fantoccini/WebDriver para regressões Rust | Interação real após hidratação e testes reproduzíveis |
| Carga | Cenários Goose em Rust contra builds release | Smoke limitado na fundação; campanha final após aceitação funcional |

Este é o registro da arquitetura vigente; desenhos anteriores permanecem no Git. A [estrutura do repositório](../architecture/repository_structure.md) distingue documentos existentes de arquivos futuros.

## Decisões de produto pendentes

O [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes) concentra as perguntas de produto e expansão. Elas não bloqueiam uma fundação com dados sintéticos, mas precisam ser resolvidas antes das features afetadas. Apresentar opções ao usuário; não inferir regras eleitorais, identidade ou garantias criptográficas.

## Fluxo escolhido

O fluxo usa a distinção do OpenSpec entre especificação vigente e mudança proposta, começando com Markdown. O CLI é opcional. Spec Kit é uma alternativa válida, mas seus artefatos e gates adicionais não são necessários ao fluxo inicial escolhido. Não instalar dois sistemas SDD concorrentes nem criar um serviço de orquestração.

Decisões de arquitetura ficam aqui; requisitos, tarefas e evidência da primeira entrega ficam na [fundação](../architecture/foundation-setup.md). Para features posteriores, atualizar o documento relevante e criar especificação separada somente quando necessário. Não criar arquivos vazios em `specs/current` alegando comportamento implementado; arquivamento deve refletir o que foi executado.

### Ciclo de uma feature

1. Especificar comportamento observável, limites e falhas; identificar decisões pendentes.
2. Revisar requisitos e ordenar tarefas por dependência. Aprovação cobre trabalho rotineiro dentro do escopo; novas decisões exigem opções ao usuário.
3. Implementar uma fatia executável e testes de comportamento, incluindo casos negativos e concorrentes críticos.
4. Executar os checks pertinentes e inspecionar os fluxos afetados em navegador real.
5. Registrar evidência e limitações, atualizar a especificação e encerrar apenas critérios satisfeitos.

Exemplo: duas submissões concorrentes de um eleitor sintético elegível não podem produzir duas cédulas aceitas. Testar tentativas distintas e replay da mesma tentativa separadamente, conforme idempotência definida. Desabilitar um botão não demonstra essa garantia.

## Instruções e harness

O AGENTS.md mantém stack, fronteiras, fluxo e requisitos de verificação, incluindo navegação por CodeGraph. Acrescentar instruções por diretório somente para uma exceção real.

O harness é o ambiente e os comandos reproduzíveis de build, inicialização e verificação; não é um serviço de produção. Usar `just` como executor fino, com a [interface única de comandos](../architecture/foundation-setup.md#contratos-futuros-dos-comandos) local e em CI. Dependências ausentes devem causar falha explícita, sem substituir o banco ou pular o navegador. Fixar toolchain Rust e lockfile; configuração de infraestrutura e código gerado para o navegador não alteram a decisão por lógica Rust.

Cada execução usa banco descartável e fixtures sintéticas próprios. Aguardar readiness por health check com timeout, sem sleep fixo. Encerrar somente processos criados pela execução. Artefatos não devem conter segredos ou dados pessoais reais.

## Estratégia de validação

### Domínio e persistência

Testes Rust verificam regras aprovadas, aritmética eleitoral e transições de estado. Usar testes de propriedades onde acrescentem confiança, como fronteiras aritméticas e transições inválidas. Integração usa TursoDB real para migrations, unicidade, atomicidade, rollback, restart e submissões concorrentes.

Testar autorização também pela rede. Eleições precisam referenciar versão do setup e eleitorado definidos conforme o PRD; testar a disputa entre aceitação de voto e encerramento. Fixtures não resolvem regras abertas.

### Navegador

Chrome DevTools MCP complementa os testes com inspeção de telas, hidratação, teclado, viewport mobile, expiração de sessão e console/rede.

Fantoccini/WebDriver executa regressões Rust com navegador/driver compatíveis. O scaffold deve comprovar uma interação após hidratação. Acrescentar login, propostas, assinaturas, votação e resultados conforme as features existam, usando seletores semânticos estáveis e esperas limitadas.

Mocks de identidade permitem testes determinísticos, devem ser rotulados e rejeitados em produção. Não validam biometria, unicidade real ou sigilo eleitoral; esses critérios exigem provider e protocolo reais.

### Carga e aceitação final

Goose usa sessões, CSRF e transporte real das server functions. Validar a resposta de aplicação: HTTP bem-sucedido não comprova aceitação do voto. Fixtures devem fornecer eleitores elegíveis distintos; repetir uma identidade não representa carga normal.

Executar carga mista, rampa até saturação, votação concentrada, duplicidades/replays e execução sustentada com restart/recuperação. O smoke da fundação verifica o harness; a campanha final depende de aceitação funcional e de navegador.

Interpretação de trabalho do RNF02: p95 de listagem abaixo de 2 segundos e de aceitação de voto abaixo de 3 segundos, com menos de 1% de falhas inesperadas, sob carga documentada. Esses limites não demonstram capacidade; carga-alvo e condições finais permanecem pendentes no PRD. Separar negativas esperadas de falhas inesperadas. Duplicidade, perda de voto confirmado e resultado inconsistente são inaceitáveis.

Registrar carga oferecida, throughput, p50/p95/p99, erros, CPU/RAM, disco, conflitos/retries e invariantes finais do banco. Identificar commit/estado do checkout, build, hardware, dataset, duração e modo do provider. Amostras vazias ou nenhum voto aceito não sustentam afirmações de latência.

Usar release. Sondagens iniciais de 10, 50 e 100 usuários simulados são experimentos, não promessa de capacidade. Avaliar saturação do gerador; antes de afirmar capacidade da implantação, repetir em VPS equivalente com gerador separado.

## Ordem de implementação futura

1. **Fundação:** scaffold Leptos/Axum/TursoDB, health check, migrations, harness/CI, um E2E e smoke de carga. Estimativa: 2–4 horas com dependências compatíveis; 4–8 horas se a integração exigir diagnóstico.
2. **Identidade e setup:** sessões, RBAC, provider sintético, bootstrap e bloqueio de leis antes da adoção. Integração real requer decisões do PRD.
3. **Propostas:** rascunhos, publicação, assinaturas, debate e transições persistidas com prazo.
4. **Votação e auditoria:** resolver protocolo/regras, aceitação e fechamento, recibos e verificação de resultados. Sem estimativa confiável para desenho e validação de segurança.
5. **Aceitação:** release, regressões de navegador, restauração e campanha final de carga. Campanha base estimada em 1–2 horas com cenários/fixtures prontos; correções são adicionais.

A fundação só termina quando seus [critérios e checklist](../architecture/foundation-setup.md#requisitos-e-critérios-observáveis), incluindo CI e smoke, tiverem evidência. Isso não demonstra prontidão biométrica, eleitoral ou operacional do produto. Nenhuma tarefa de implementação foi executada.

## Referências

- [OpenSpec: fluxo de mudança](https://openspec.dev/docs/quickstart)
- [Spec Kit: fluxo SDD](https://github.github.io/spec-kit/quickstart.html)
- [Orientações para AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
- [Leptos: testes](https://book.leptos.dev/testing.html)
- [Fantoccini: cliente Rust WebDriver](https://github.com/jonhoo/fantoccini)
- [Goose: testes de carga](https://book.goose.rs/)
