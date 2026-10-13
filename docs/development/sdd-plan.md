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
| Feature flags | Configuração por instância lida na inicialização; mudanças valem após reiniciar a aplicação | Configuração no banco local; ver [contrato do servidor](../architecture/backend-architecture.md#feature-flags-por-instância) |
| Cadastro e território | E-mail, senha, nome e endereço obrigatórios; sem documento nacional | Endereço inicial define cidade/estado/país de participação e fica bloqueado para alteração nesta versão |
| Busca de endereço | BrasilAPI por CEP, com provedor selecionado por feature flag da instância | Preenchimento automático somente com os campos do retorno, sem número/complemento; nomes preservados no banco; contrato no [guia de banco](../architecture/database-structure.md#endereço-e-brasilapi) |
| Login | E-mail + senha, com verificação facial e TOTP; sem confirmação de e-mail nesta versão | Sessão só após validar os fatores, sem manter conectado entre usos; expiração após 15 minutos sem atividade; sessão persistida no TursoDB com cookie seguro; recuperação de TOTP por códigos de uso único; recuperação de senha por rosto + TOTP, sem e-mail; detalhes de renovação na proposta delegada abaixo |
| Motor facial | Aproveitar somente o serviço Python do auth-face na mesma VPS | Rust mantém contas, sessões, autorização e TursoDB; correções e validação no [guia de identidade](../architecture/auth-rbac-flow.md) |
| Uso | Exclusivamente online, sem votação ou coleta de assinaturas em meio físico | PWA instalável não implica funcionamento offline |
| SDD | Especificações Markdown enxutas inspiradas no OpenSpec, sem CLI obrigatório | Uma feature ou mudança por vez, com requisitos, tarefas e evidência |
| Instruções de agentes | Um AGENTS.md curto na raiz | Arquivos locais somente para exceções reais |
| Harness de validação | Justfile fino com comandos locais e de CI | Contratos na [fundação](../architecture/foundation-setup.md#contratos-futuros-dos-comandos); ainda não implementados |
| Navegador | Chrome DevTools MCP para inspeção; Fantoccini/WebDriver para regressões Rust | Interação real após hidratação e testes reproduzíveis |
| Carga | Cenários Goose em Rust contra builds release | Smoke limitado na fundação; campanha final após aceitação funcional |

Este é o registro da arquitetura vigente; desenhos anteriores permanecem no Git. A [estrutura do repositório](../architecture/repository_structure.md) distingue documentos existentes de arquivos futuros.

## Decisões de produto pendentes

O usuário delegou as decisões restantes para revisão posterior. As propostas abaixo são escolhas do agente, não aprovação individual do usuário nem implementação. O [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes) relaciona os temas a estas propostas e aos bloqueadores de validação. Não converter proposta ou resultado de mock em garantia de produção.

## Decisões delegadas para revisão

Status de todas as escolhas desta seção: **propostas pelo agente, aguardando revisão do usuário**. Preservam as decisões expressas de cadastro, autenticação, endereço imutável e uso online. Não autorizam implementação.

| Área | Escolha provisória |
| --- | --- |
| Implantação | Brasil, idioma inicial pt-BR, fuso de apresentação America/Sao_Paulo e armazenamento de instantes em UTC. Outros fusos por configuração. Desenvolvimento 127.0.0.1:3000; produção exige domínio e VPS reais fornecidos pelo operador, sem inventar recursos contratados |
| Operação inicial | Planejar VPS Linux com 4 vCPU, 16 GiB RAM e SSD, um processo Rust, um motor Python privado e proxy HTTPS. Recursos são ponto de partida para medir, não capacidade demonstrada |
| Escopo de entrega | Fundação primeiro, depois cadastro/sessão/configuração/propostas/debate. Demonstrações eleitorais usam dados sintéticos. Uso eleitoral real fica bloqueado até validações de identidade, território, protocolo e operação |
| Interface | Português brasileiro inicialmente, textos preparados para i18n; CSS simples, sem biblioteca visual obrigatória. PWA instalável online, cache apenas de assets públicos versionados; dados pessoais sem cache. Inglês/espanhol, push, exportação externa, API pública externa e administração global ficam para expansão |
| Identidade | E-mail aparado e convertido a minúsculas para unicidade por instância; sem aliases especiais por provedor. Sem idade mínima inventada ou coleta de nascimento nesta versão demonstrativa. Sem CPF; unicidade de pessoa/cidadania/residência não é afirmada e bloqueia uso eleitoral real |
| Endereço | Aceitar CEP genérico se cidade/estado forem válidos; bairro/logradouro vazios são preservados. Resolver jurisdição por código IBGE quando disponível, senão por cidade+UF em catálogo local versionado, sem correspondência aproximada. Ausência/ambiguidade bloqueia cadastro territorial. País vem da instância; endereço não é prova de residência |
| Credenciais | Senhas de 12 a 128 caracteres, sem regras artificiais de composição ou troca periódica. Argon2id com salt aleatório, base de 19 MiB, 2 iterações e paralelismo 1, ajustada por medição. Troca/redefinição de senha revoga todas as sessões e exige novo login |
| Sessão | Cookie __Host-dwc_session em produção, HttpOnly, Secure, SameSite=Lax, Path=/, sem Domain, Max-Age ou Expires; proteção CSRF nos endpoints mutáveis. ID aleatório de 32 bytes, somente hash no banco. Limite absoluto de 8 horas além dos 15 minutos de inatividade. Navegação autenticada e ações explícitas renovam atividade; polling/health checks não renovam. Gerar novo ID após login/reautenticação que altere privilégio |
| TOTP e recuperação | TOTP de 6 dígitos/30 segundos, janela de um passo para cada lado e bloqueio de replay do mesmo passo. Dez códigos de recuperação com 128 bits aleatórios, exibidos uma vez e guardados apenas como hashes. Regeneração exige senha+rosto+TOTP e revoga os códigos anteriores. Após código de recuperação, reenrolar TOTP antes de ações críticas. Perda simultânea de senha/TOTP/códigos ou incapacidade de validar rosto não tem bypass administrativo nesta versão |
| Proteção de autenticação | Base de 5 falhas em 15 minutos por conta e limitação adicional por IP, com resposta genérica e sem bloqueio permanente automático. Reautenticação crítica de rosto+TOTP vinculada à sessão, ação e recurso, válida por 5 minutos e consumida uma vez; falhas não autorizam a operação |
| Motor facial | Reutilizar apenas Python do auth-face; imagens em memória, sem arquivos temporários, serviço privado e requisições limitadas. Vivacidade validada no servidor, vinculada a desafio/nonce de uso único. Calibrar matching, falsos resultados e ataques de apresentação antes de habilitar uso real; a piscada do frontend e o threshold 0,70 atual não são aceitos como evidência suficiente |
| Flags | Painel do Admin País grava catálogo conhecido no banco, com auditoria e aviso “aplicação pendente de reinício”. Só o operador da VPS reinicia o serviço, fora do painel. Valores ausentes desabilitam features opcionais; flags nunca desligam autenticação/autorizações. BrasilAPI ligado apenas na instância BR; um provedor de endereço compatível. Cadastro/login/perfil são base; proposta/assinatura/debate dependem de setup adotado; votação e verificadores privados permanecem desligados até validação |
| Reinício | Parar novas gravações, aguardar requisições em curso por até 30 segundos e iniciar com configuração validada. Não há troca de flags durante uma operação. Replays usam idempotência e sessões preservam expiração/revogação. Exigir testes de interrupção e recuperação antes de prometer esse comportamento |
| Auditoria | Sem blockchain na versão inicial. Eventos append-only com minimização, hash encadeado e checkpoints assinados exportados fora da VPS; não afirmar resistência ao operador só com uma cadeia local. Auditor é credenciado manualmente pelo Admin País e só lê evidências no seu escopo, sem cédulas privadas |
| Persistência | UUID/datas ISO-8601 UTC em TEXT, booleanos INTEGER 0/1, enumerações TEXT com CHECK e percentuais em pontos-base inteiros. Migrations SQL versionadas com histórico/checksum; JSON de ibge serializado preservando chaves. Tudo sujeito a integração com TursoDB real |
| Proteção e retenção | Chaves fora do Git/banco, arquivos de acesso restrito; criptografar templates/segredos e backups, com rotação versionada e teste de restauração. Logs operacionais sem segredos/biometria por 30 dias; dados da conta enquanto ativa; remoção/anonimização mediante política revisada antes de produção. Histórico público eleitoral minimizado por 10 anos, sem estender essa retenção a biometria/IP indiscriminadamente |
| Backup e atualização | Backup consistente diário completo e horário incremental/novo snapshot suportado pelo motor, 7 diários e 4 semanais, criptografados em armazenamento separado. Metas RPO de 1 hora e RTO de 4 horas, sujeitas a restauração real. Atualizações por release versionado, migrations não destrutivas e rollback de binário apenas compatível com o schema; sem downgrade/reset automático de banco |
| Carga inicial | Aceitação experimental com 50 usuários sintéticos por 15 minutos, mistura 95% leituras/5% gravações, release e banco isolado. p95 de listagem <2s, de aceite <3s e <1% falhas inesperadas; invariantes sem votos duplicados/perdidos. Medir gerador separado antes de declarar capacidade; não prometer milhões de votos |

### Regras eleitorais e de propostas para revisão

Estas regras fecham ambiguidades para futuros testes, **não são legislação ou regras de produção aprovadas**. Percentuais/prazos do PRD continuam sugestões configuráveis por plebiscito; não codificar constantes brasileiras para todos os países.

- Participação no município, estado e país ligados ao endereço inicial; leituras públicas de outras jurisdições permitidas. Snapshot de contas elegíveis e versão do setup na abertura da coleta e da votação, materializado e imutável durante cada fase. Alterações posteriores de status contam para novas eleições, salvo revogação de segurança explicitamente auditada; não criar eleitores retroativamente.
- Quórum conta SIM+NÃO+ABSTENÇÃO sobre o snapshot; mínimo de votos/assinaturas é ceil(percentual × eleitorado). Maioria normal/urgente exige SIM>NÃO. Setup/meta-lei adota provisoriamente SIM>60% dos votos válidos, isto é floor(0,60 × válidos)+1, substituindo a expressão ambígua “60% + 1”. Zero votos válidos ou empate rejeita, mantendo o motivo; eleitorado vazio não abre eleição.
- Um setup por jurisdição, inicialmente herdado do nível nacional até adoção local; nunca alterar regras de eleição em curso. Admin País inicia o plebiscito nacional usando pacote sugerido, sem poder adotá-lo unilateralmente; rejeição/falta de quórum mantém leis bloqueadas e permite nova proposta de setup. Meta-leis têm coleta/debate como lei normal e votação de 30 dias; requisitos especiais de quórum/maioria do PRD. Vigência após publicação do resultado, só em eleições futuras.
- Quota é consumida atomicamente na publicação, por ano civil do fuso da instância; rascunho não consome, cancelamento não devolve. Texto fica imutável após publicação; emenda é sugestão para nova versão/proposta, sem mudar texto já apoiado. Autor pode retirar rascunho ou coleta antes da votação, com motivo/histórico, encerrando em Arquivada; após abrir votação não pode retirar. Arquivamento administrativo somente de fase terminal, preservando histórico.
- Uma assinatura ativa por conta/proposta/versão, retirada/reassinatura somente na coleta, com apuração do limiar em transação. Ao atingir limiar, fechar coleta e entrar na próxima fase; retirar assinatura depois disso não é permitido. Prazos são instantes UTC persistidos; encerramento prevalece quando horário do servidor atinge o prazo. Em idempotência, a mesma chave/carga devolve resultado anterior; mesma chave/carga diferente gera conflito.
- Conteúdo duplicado gera aviso por título/texto normalizado, sem impedir publicação automaticamente. Comentários têm edição com histórico; denúncia registra motivo e moderação remove apenas da visão pública, com motivo/auditoria. Não há votação de emendas ou upvote/downvote nesta primeira versão; bloqueio de thread exige motivo e não altera resultado eleitoral.

### Bloqueadores de validação que decisões não eliminam

1. Unicidade de pessoa, elegibilidade real e residência não são provadas pelo cadastro sem documento, e-mail não confirmado, CEP e matching facial. Decisão: limitar a demonstração a contas/dados sintéticos e bloquear uso eleitoral real até evidência verificável; não inventar uma prova de identidade.
2. Protocolo privado: usar Helios como referência de avaliação, não integrar ou reimplementar sua criptografia nesta fase. A [FAQ do próprio Helios](https://vote.heliosvoting.org/faq) desaconselha eleições públicas de alto impacto. Decisão: nenhuma criptografia própria ou promessa de sigilo; fluxo privado, custódia distribuída de chaves, recibo e unicidade público/privado só serão implementados após especificação especializada e validação. Referência não é escolha de protocolo para produção.
3. Hardware contratado, domínio, remetente de e-mail, destino/chaves de backup, catálogos territoriais e versões exatas das dependências são entradas de implantação/scaffold; não existem valores reais confirmados. Decisão: configuração obrigatória validada, falha explícita quando ausente, sem segredos ou endereços inventados.
4. Privacidade, retenção, direitos de exclusão, vivacidade e acessibilidade precisam de revisão apropriada e testes reais; não declarar conformidade jurídica, WCAG ou segurança a partir deste documento.

Fontes técnicas: [OWASP: senhas](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html), [OWASP: sessões](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html) e [MDN: cookie de sessão e restauração do navegador](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie). As fontes orientam a proposta; não validam a aplicação.

## Fluxo escolhido

O fluxo usa a distinção do OpenSpec entre especificação vigente e mudança proposta, começando com Markdown. O CLI é opcional. Spec Kit é uma alternativa válida, mas seus artefatos e gates adicionais não são necessários ao fluxo inicial escolhido. Não instalar dois sistemas SDD concorrentes nem criar um serviço de orquestração.

Decisões de arquitetura ficam aqui; requisitos, tarefas e evidência da primeira entrega ficam na [fundação](../architecture/foundation-setup.md). Para features posteriores, atualizar o documento relevante e criar especificação separada somente quando necessário. Não criar arquivos vazios em `specs/current` alegando comportamento implementado; arquivamento deve refletir o que foi executado.

### Ciclo de uma feature

1. Especificar comportamento observável, limites e falhas; identificar decisões pendentes.
2. Revisar requisitos e ordenar tarefas por dependência. Aprovação cobre trabalho rotineiro dentro do escopo; seguir as propostas delegadas; divergências materiais devem ser identificadas para revisão, sem declarar aprovação inexistente.
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
