# Persistência e modelo de dados

Atualizado em 2026-10-12. Banco aprovado: TursoDB local pela crate Rust `turso`. Não há schema executável, migrations ou banco da aplicação. Este é um modelo conceitual a detalhar por feature; ver [PRD](../prd/prd.md) e [plano SDD](../development/sdd-plan.md).

## País, processo e armazenamento

Uma instância de aplicação possui um banco persistente local do país configurado. Cada país terá implantação, VPS e banco independentes. O desenho PostgreSQL por schema e catálogo global foi substituído; não executar os SQLs antigos como migrations TursoDB.

No país, jurisdições formam uma hierarquia nacional/estadual/municipal. Queries e casos de uso precisam validar esse escopo e elegibilidade. O banco de uma instância não deve ser escolhido a partir de um país arbitrário enviado pelo navegador.

Dados de produção, desenvolvimento e teste usam caminhos separados. Testes criam bancos descartáveis com o motor real e nunca resetam banco persistente do usuário. Somente o processo da aplicação possui o banco de produção; backup e manutenção precisam de procedimento compatível com esse modelo.

## Convenções e decisões físicas pendentes

Manter nomes em snake_case e identificadores lógicos UUID para entidades principais. Representação SQL de UUID, datas, enumerações, percentuais e constraints deve ser definida e validada no motor real durante migrations; não pressupor tipos/funções do banco anterior.

Persistir marcos de tempo de forma consistente e apresentar datas conforme locale/fuso do país. Percentuais e arredondamentos exigem regras exatas antes da implementação eleitoral. Campos de atualização e exclusão devem corresponder ao ciclo de vida da entidade, sem aplicar soft delete indiscriminadamente a todos os registros.

## Identidade, perfis e autorização

Os campos abaixo descrevem necessidades, não um schema final nem dados que devam ser expostos por endpoints.

| Entidade conceitual | Dados e vínculos necessários |
| --- | --- |
| Identidade/cidadão | ID, dados de cadastro mínimos, status, jurisdição/residência e referência protegida ao provider |
| Perfil | user_id, nome de exibição, foto/bio autorizadas e visibilidade pública/privada |
| Preferências | Visibilidade padrão do voto, idioma e notificações |
| Sessão/credenciais | Estado necessário a expiração, revogação e recuperação segundo contrato de autenticação ainda aberto |
| Segundo fator | Estado de configuração e segredo/referência protegidos; recuperação ainda a especificar |
| Jurisdição | ID, nome, tipo nacional/estadual/municipal, parent_id e status |
| Roles/permissões | Papéis e permissões, vínculos de usuário/role e role/permissão conforme políticas |
| Reautenticação | Ação/recurso autorizado, provider, resultado, validade e consumo; associação com cédula privada depende do protocolo |

Separar dados pessoais de projeções públicas. Não armazenar imagem biométrica bruta. A proposta de `biometric_hash UNIQUE` não é uma solução especificada para matching ou unicidade de pessoas. Unicidade documental/e-mail e residência entre jurisdições precisam de escopo definido, sem permitir cadastro duplicado apenas por mudança territorial.

As entidades globais `countries`, `platform_admins` e `global_auditors` do desenho anterior não são necessárias à fundação. Cadastro/autoridade global continua uma questão de expansão do PRD, sem banco global ativo agora.

## Setup, propostas e participação

| Entidade conceitual | Dados e vínculos necessários |
| --- | --- |
| Configuração de setup | Versão, jurisdição, status, percentuais, prazos, limites e referência ao plebiscito/meta-lei que a aprova |
| Histórico do setup | Versões/valores anteriores e novos, proposta de origem e momento de vigência |
| Eleitorado de eleição | Snapshot de elegibilidade, referência territorial e versão de regras; materialização a especificar |
| Proposta | ID, título, resumo, texto/versão, autor, jurisdição, tipo, status, meta de assinaturas e prazos |
| Assinatura | Proposta e cidadão elegível, versão apoiada, data e verificação necessária |
| Comentário | Proposta, autor, parent_id, conteúdo, status/moderação e datas |
| Emenda | Proposta/texto de referência, autor, título/conteúdo e estado de processamento |
| Notificação | Destinatário, evento, título/mensagem, leitura e data; preferências e entrega in-app/e-mail |

Tipos de proposta: `normal_law`, `urgent_law`, `setup_change`, `initial_setup`. Status de produto: `draft`, `collecting_signatures`, `in_debate`, `in_voting`, `approved`, `rejected`, `quorum_not_reached`, `expired`, `archived`.

Notificações previstas incluem criação de proposta, abertura de votação, lembrete de prazo e aprovação/rejeição. Entrega de e-mail requer política de retry e deduplicação especificada antes de prometer esse fluxo.

Setup e histórico devem existir antes das leis. Cada eleição precisa de regras/eleitorado estáveis, com aplicação de mudança de setup definida. A forma de guardar snapshots, transições e versão apoiada será decidida por feature; o modelo não resolve retirada de assinatura, edição após publicação ou empate.

## Votos, recibos e resultados: bloqueados pelo protocolo

O produto prevê SIM/NÃO/ABSTENÇÃO, voto privado por padrão e público por consentimento. A arquitetura de dados definitiva depende de um protocolo eleitoral estabelecido e validado, ainda não selecionado.

| Necessidade | O que falta definir |
| --- | --- |
| Cédula/aceitação | Representação pública/privada, prova de elegibilidade e ponto de persistência |
| Unicidade | Mecanismo comum para impedir voto duplicado, inclusive alternância público/privado |
| Idempotência | Replay da mesma tentativa versus tentativa distinta do mesmo eleitor |
| Recibo | O que comprova, como verificar inclusão e quais informações revela |
| Resultado | Snapshot do eleitorado, contagens, regras/versões, quórum, maioria e encerramento verificável |

Não considerar duas constraints separadas para voto público e hash privado como garantia conjunta de um voto por pessoa. O documento anterior também vinculava votos a `biometric_reauth_id` e recibos a `user_id`; esses vínculos não estão aprovados para cédulas privadas e precisam de análise de correlação no protocolo.

Um campo `zkp_proof`, um hash anônimo ou ocultar `user_id` não especifica sigilo. Não definir DDL eleitoral definitivo ou anunciar voto privado implementado com base nesses placeholders. O resultado `tie` aparece nos wireframes, mas sua regra e relação com os status do PRD permanecem abertas.

## Invariantes e transações a verificar

1. Uma assinatura válida por cidadão/proposta, com semântica de retirada/reassinatura definida antes da feature.
2. Publicação/aceitação condicionadas a setup adotado, status, escopo territorial, autoria e quota aplicável.
3. Eleição com versão de setup e eleitorado definidos; votos aceitos não podem ser perdidos nem duplicados por replay/concorrência.
4. Aceitação e fechamento coordenados transacionalmente; resultados consistentes com registros persistidos após falha/restart.
5. Auditoria/recibo e efeito de negócio consistentes conforme protocolo, sem registrar um sucesso que não foi persistido.

Usar constraints e transações do motor real conforme a regra, com testes de rollback, concorrência, restart e falha. Índices devem apoiar consultas por status, tipo, jurisdição, autoria, proposta e data conforme uso concreto; leituras paginadas e limites são necessários. Contadores e tabelas de resultado derivados precisam de critérios de consistência e reconstrução.

## Auditoria e retenção

Eventos críticos incluem cadastro/login, proposta, assinatura, voto, mudança de setup, papéis e acesso de auditor. Definir campos mínimos e acesso público/restrito por tipo de evento. Não associar automaticamente conteúdo privado a user_id, IP, user-agent, horário exato ou reautenticação identificável.

O produto exige preservação auditável de votos aceitos e eventos críticos. Correções devem ser explicitadas e rastreáveis; não apagar silenciosamente histórico eleitoral. Soft delete em conteúdo administrativo/comentários não resolve retenção de dados pessoais.

Cadeia de hashes é uma técnica candidata à verificação de integridade, não uma demonstração de imutabilidade perante um operador privilegiado. Modelo de ameaça, verificação externa, custódia de chaves e política de retenção ainda precisam ser definidos. A menção de blockchain no PRD não escolhe uma implementação.

Dados sensíveis incluem documentos, biometria, credenciais/segredos TOTP, sessão, IP, user-agent e histórico privado. Definir proteção em repouso, gestão/rotação/recuperação de chaves, permissões de arquivo, exportação e backup. Não presumir que declarar AES-256 significa que o banco já os protege.

## Migrations, fixtures e operação

Toda mudança estrutural usa migration versionada, separada de fixtures. Fixtures sintéticas podem incluir país/jurisdição, permissões e setup de teste, sem adotar implicitamente o primeiro país ou regras de produção. Não criar admin/credencial fixa de produção por seed.

Versionar/aplicar migrations de forma reproduzível, tratando execução repetida, falha parcial e compatibilidade com dados existentes. Não alterar schema manualmente nem destruir dados para fazer um teste passar.

Backup diário e retenção de dez anos são metas propostas do PRD, com política por categoria e objetivos de perda/tempo de recuperação ainda abertos. Validar um procedimento consistente com o motor real e provar restauração em caminho isolado; copiar um arquivo sem validação não é evidência de recuperação.

## Ordem e evidência

Fundação: migration e dado sintético mínimo para persistência/restart. Depois identidade/sessão/RBAC e setup; propostas/assinaturas/debate; votação/recibos/resultados após resolver protocolo; notificações e consultas conforme features.

Nenhuma entidade ou constraint deste guia está implementada. Evidência inicial fica na [checklist da fundação](foundation-setup.md), sem novo relatório de análise. Validar com TursoDB real; testes mock não comprovam atomicidade, migração ou persistência.
