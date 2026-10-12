# Arquitetura do servidor

Atualizado em 2026-10-12. Stack aprovada: Rust, Leptos SSR, Axum/Tokio e TursoDB local. Não há servidor implementado. As decisões estão no [plano SDD](../development/sdd-plan.md) e o produto no [PRD](../prd/prd.md).

## Processo e interfaces

Uma aplicação por país serve HTML SSR, assets e server functions Leptos. Axum integra o transporte HTTP e Tokio o runtime. O mesmo processo possui o banco persistente local; o início não exige um serviço de API separado.

As server functions são endpoints públicos de rede em termos de superfície de ataque. Cada operação protegida deve obter contexto de sessão no servidor, validar entradas e delegar à aplicação. Ações administrativas e eleitorais não podem confiar em role, eleitorado, timestamp ou configuração fornecidos pelo cliente.

O PRD conserva as capacidades de leitura pública e operações autenticadas. Uma API REST externa completa não integra a fundação; seu contrato e consumidores serão especificados se solicitados. Não implementar em paralelo duas interfaces completas para as mesmas features.

## Camadas

| Camada | Responsabilidade |
| --- | --- |
| Domínio | Tipos, invariantes, aritmética eleitoral e transições aprovadas |
| Aplicação | Casos de uso, autorização por recurso/território, coordenação transacional e idempotência |
| Infraestrutura | TursoDB, migrations, provedores de identidade, configuração e integração de operação |
| Features/HTTP | SSR, server functions, validação de transporte e erros públicos |

A organização de módulos está no [guia do repositório](repository_structure.md). Interfaces e abstrações devem atender testes ou integrações concretas; não repetir pastas e camadas vazias para cada entidade.

## Responsabilidades por funcionalidade

| Área | Comportamento pretendido |
| --- | --- |
| Identidade e sessão | Cadastro pendente, prova de identidade, login, 2FA, revogação, recuperação e reautenticação |
| Perfis | Dados pessoais separados de perfil público, preferências de privacidade e notificações |
| Jurisdições | Hierarquia territorial do país configurado e verificação de elegibilidade |
| RBAC | Papéis, permissões e políticas por recurso; negar por padrão |
| Setup | Plebiscito inicial, versionamento de regras, meta-leis e bloqueio de leis antes da adoção |
| Propostas e assinaturas | Rascunho, autoria, publicação, quotas, coleta, prazos e unicidade |
| Debate | Comentários encadeados, sugestões/emendas e moderação auditável |
| Votação | Elegibilidade, SIM/NÃO/ABSTENÇÃO, privacidade, aceitação, fechamento e apuração |
| Auditoria | Eventos apropriados à privacidade, acesso de auditores e verificação de integridade |
| Dashboard e notificações | Consultas paginadas, estatísticas públicas e notificações in-app/e-mail conforme preferências |

Não existe catálogo de países ou autoridade global necessária ao runtime inicial. As funções de Admin Master dependem das [decisões de expansão do PRD](../prd/prd.md#13-decisões-de-produto-pendentes), sem banco global ou acesso automático a outros países.

## Autorização e setup

Aplicar sessão, status do cidadão, permissão, jurisdição, propriedade do recurso, fase da proposta e reautenticação na aplicação. O frontend apenas comunica essas condições.

Administração operacional não autoriza mudar quórum, prazos ou percentuais por edição direta. A mudança eleitoral exige meta-lei aprovada e histórico de versão. Bootstrap e autoridade operacional dependem das [decisões de setup e expansão](../prd/prd.md#13-decisões-de-produto-pendentes).

## Persistência e concorrência

Migrations, queries e transações usam o motor TursoDB real. O modelo conceitual e suas limitações estão no [guia de banco](database-structure.md).

1. Publicação e assinatura devem verificar setup, elegibilidade, fase e quota junto à gravação correspondente.
2. Aceitação de voto deve coordenar duplicidade, idempotência e fechamento. Duas tentativas distintas e replay da mesma tentativa precisam de critérios diferentes.
3. A eleição precisa referenciar uma versão do setup e um snapshot do eleitorado, conforme regras ainda a decidir.
4. Transições com prazo devem persistir seus marcos e retomar após restart; não depender apenas de timer em memória.
5. Sucesso só pode ser informado após persistência da operação. Falha de resposta após commit exige consulta/retry com semântica definida, sem novo efeito indevido.

O protocolo de voto privado é bloqueador da persistência eleitoral definitiva. Não atribuir garantias reais a um modelo demonstrativo.

## Segurança, erros e observabilidade

Validar configuração na inicialização e rejeitar provedor de identidade de teste em produção. Usar loopback no desenvolvimento, limites de entrada e requisições, proteção de sessão/CSRF apropriada ao transporte e mensagens sem dados sensíveis. A estratégia concreta de credenciais e cookies/tokens permanece a especificar no [guia de autenticação](auth-rbac-flow.md).

Distinguir entrada inválida, sessão expirada, acesso negado, fase encerrada, conflito, limitação de tentativas e falha interna. Erros de banco ou de provedor não devem virar sucesso ou resultado eleitoral vazio. Não expor stack traces, documentos, segredos ou material biométrico nas respostas.

Registrar falhas, conflitos e transições com dados mínimos. Não registrar automaticamente identidade, IP e horário associados ao conteúdo de voto privado. Auditoria e métricas precisam respeitar o protocolo e a política de retenção; TLS/AES/hash em documentos não são garantias implementadas.

## Validação e sequência

A [fundação](foundation-setup.md) deve comprovar SSR/WASM, persistência após restart e interação hidratada. Depois testar permissões pela rede, fases inválidas, concorrência, rollback e retomada de transições em bancos descartáveis.

Implementar sessão/RBAC e setup antes das leis. Propostas, assinaturas e debate podem avançar conforme suas regras aprovadas. Votação com privacidade só avança após o desenho eleitoral; carga final e restauração vêm após aceitação funcional.
