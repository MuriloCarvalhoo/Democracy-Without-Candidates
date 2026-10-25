# Persistência e modelo de dados

Atualizado em 2026-10-25. Banco aprovado: TursoDB local pela crate Rust `turso` 0.8.3. O schema executável está em `migrations/` e é aplicado na inicialização com checksum; este guia mantém o modelo conceitual e as restrições. Ver [PRD](../prd/prd.md) e [plano SDD](../development/sdd-plan.md).

| Migration | Conteúdo |
| --- | --- |
| `0001_foundation` | Flags, catálogo territorial, cadeia de auditoria append-only, contador sintético |
| `0002_identity_core` | Contas, papéis, endereço imutável, sessões, reautenticação, notificações |
| `0100_identity` | Credenciais cifradas, desafios, códigos de recuperação, limites, preferências |
| `0200_setup_proposals` | Setup versionado, snapshots de eleitorado, propostas, assinaturas, idempotência |
| `0300_debate` | Comentários, revisões, denúncias, moderação, bloqueios, emendas |
| `0400_voting` | Cédulas públicas demonstrativas e resultados imutáveis |
| `0450_audit_notifications` | Checkpoints, preferências e agendamento de notificações |
| `0500_iam` | Políticas IAM (`iam_policies`, gerenciadas por trigger), grupos e anexos usuário/grupo; converte `user_roles` (CITIZEN→`Cidadao`, AUDITOR→`Auditor`, COUNTRY_ADMIN→`AdministratorAccess`) e remove a tabela |
| `0520_performance` | Índices de expiração (`face_challenges`, `auth_challenges`, `auth_failures`) e `users(country_id, status)`; tabela `ballot_counts` (cédulas aceitas por proposta, incrementada na transação do voto, preenchida a partir de `ballots`) |

Verificado no motor real: CHECK, UNIQUE, FK, triggers, RETURNING, ON CONFLICT, `VACUUM INTO` e `PRAGMA integrity_check`. Dois escritores simultâneos retornam Busy (escritas passam pelo escritor único `Db::write()`); leituras reaproveitam conexões de um pool (`Db::read()`, até 32 ociosas, descartadas se devolvidas dentro de transação) e veem o último commit em WAL, sem esperar o escritor e sem leitura suja; triggers com cadeias longas de `OR` estouram a pilha em debug.

## País, processo e armazenamento

Uma instância de aplicação possui um banco persistente local do país configurado. Cada país terá implantação, VPS e banco independentes.

Jurisdições formam a hierarquia configurada para o país, inicialmente nacional/estadual/municipal no Brasil. Queries e casos de uso precisam validar esse escopo e elegibilidade. O banco de uma instância não deve ser escolhido a partir de um país arbitrário enviado pelo navegador.

Dados de produção, desenvolvimento e teste usam caminhos separados. Testes criam bancos descartáveis com o motor real e nunca resetam banco persistente do usuário. Somente o processo da aplicação possui o banco de produção; backup e manutenção precisam de procedimento compatível com esse modelo.

## Configuração de funcionalidades

Entidade conceitual `feature_flags`: chave de funcionalidade e estado habilitado/desabilitado, próprios do banco da instância. As flags são lidas na inicialização; mudanças no banco ficam pendentes de reinício da aplicação. Proposta: painel local do Admin País, com autoria/data auditadas e indicação de reinício pendente, conforme o SDD. O catálogo de chaves é definido pela aplicação; uma chave arbitrária no banco não cria funcionalidade nem permissão. Alterações devem ter autoria/data auditáveis, conforme o [contrato do servidor](backend-architecture.md#feature-flags-por-instância).

Flags implementadas em `src/infrastructure/flags.rs`; `voting.private` nunca pode ser ligada nesta versão.

## Convenções e decisões físicas pendentes

Manter nomes em snake_case e identificadores lógicos UUID para entidades principais. Representação proposta no SDD: UUID/datas UTC em TEXT, booleanos INTEGER, enums com CHECK e percentuais em pontos-base. Verificar com o motor real durante migrations; não pressupor tipos/funções de outro banco.

Persistir marcos de tempo de forma consistente e apresentar datas conforme locale/fuso do país. Percentuais e arredondamentos exigem regras exatas antes da implementação eleitoral. Campos de atualização e exclusão devem corresponder ao ciclo de vida da entidade, sem aplicar soft delete indiscriminadamente a todos os registros.

## Identidade, perfis e autorização

Os campos abaixo descrevem necessidades, não um schema final nem dados que devam ser expostos por endpoints.

| Entidade conceitual | Dados e vínculos necessários |
| --- | --- |
| Identidade/cidadão | ID, e-mail, nome e endereço obrigatórios, referências à cidade/estado/país derivadas do endereço, status e referência protegida ao provider; sem documento nacional |
| Perfil | user_id, nome de exibição, foto/bio autorizadas e visibilidade pública/privada |
| Preferências | Visibilidade padrão do voto, idioma e notificações |
| Sessão/credenciais | Hash de senha com salt, sem senha em texto puro; estado de sessão persistido no TursoDB da instância, associado ao identificador opaco do cookie, com última atividade, expiração e revogação preservadas após reinício; schema/migrations ainda a detalhar, sem manter conectado entre usos |
| Segundo fator | Estado de configuração e segredo/referência protegidos; hashes de códigos de recuperação TOTP e estado de consumo de uso único, com atualização atômica |
| Jurisdição | ID, nome, tipo nacional/estadual/municipal, parent_id e status |
| Políticas IAM | Documentos JSON validados no servidor, grupos e anexos usuário/grupo↔política; avaliação em [autorização](auth-rbac-flow.md#autorização-no-modelo-iam) |
| Reautenticação | Ação/recurso autorizado, provider, resultado, validade e consumo; associação com cédula privada depende do protocolo |

Separar dados pessoais de projeções públicas. Não armazenar imagem biométrica bruta. A proposta de `biometric_hash UNIQUE` não é uma solução especificada para matching ou unicidade de pessoas. E-mail identifica a conta na instância, sem documento nacional. O endereço inicial e seu vínculo territorial não podem ser alterados nesta versão. Persistência e casos de uso devem preservar esse bloqueio; normalização/formato de e-mail e validação do endereço precisam de critérios definidos; e-mail informado não é verificado e sua confirmação não bloqueia ativação nesta versão; unicidade de e-mail não garante unicidade de pessoas.

Entidades de administração global não integram a fundação; dependem das [decisões de expansão do PRD](../prd/prd.md#13-decisões-de-produto-pendentes).

## Endereço e BrasilAPI

O Brasil usa BrasilAPI para preencher o endereço a partir do CEP. Como escolha técnica inicial, usar `GET https://brasilapi.com.br/api/cep/v1/{cep}`, suficiente para endereço sem geolocalização. Preservar os nomes do retorno no modelo persistido:

| Campo | Conteúdo |
| --- | --- |
| `cep` | Código postal como texto, preservando zeros à esquerda |
| `state` | Sigla do estado |
| `city` | Nome do município |
| `neighborhood` | Bairro; pode vir vazio em CEPs genéricos |
| `street` | Logradouro; pode vir vazio em CEPs genéricos |
| `service` | Fonte usada internamente pelo BrasilAPI, como `open-cep`; não sobrescrever com o nome do adaptador da aplicação |
| `ibge` | Objeto com `city` e `state`: códigos IBGE como texto ou null, mantendo chaves aninhadas |

O objeto `ibge` será preservado serializado, com representação SQL validada na migration; não há schema executável. Referências internas de cidadão/jurisdição permanecem separadas desses campos. O país vem da configuração da VPS: BrasilAPI não retorna país, número ou complemento. Não adicionar campos de número ou complemento nesta versão; o endereço usa somente os campos do retorno.

Outros provedores serão adaptadores selecionados pelas flags, mapeando seus retornos para o mesmo modelo onde aplicável, sem impor CEP de oito dígitos ou códigos IBGE a outros países. BrasilAPI desligado não altera endereços já cadastrados nem autoriza editá-los.

Fonte: [implementação oficial do retorno CEP](https://github.com/BrasilAPI/BrasilAPI/blob/0104c49c94017e47560615c8713ba7686ae2e381/services/cep/cep.js). Em 2026-10-12, uma consulta pública ao CEP `89010025` confirmou esses sete campos, incluindo `ibge`. Isso verifica o serviço externo em uma amostra, não a integração da aplicação.

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

Setup e histórico devem existir antes das leis. Cada eleição precisa de regras/eleitorado estáveis, com aplicação de mudança de setup definida. Snapshots materializados por fase, texto imutável após publicação, retirada de assinatura só na coleta e empate rejeitado são propostas no SDD, não implementações. Schema/transações exigem testes reais.

## Votos, recibos e resultados: bloqueados pelo protocolo

O produto prevê SIM/NÃO/ABSTENÇÃO, voto privado por padrão e público por consentimento. A arquitetura de dados definitiva depende de um protocolo eleitoral estabelecido e validado, ainda não selecionado.

| Necessidade | O que falta definir |
| --- | --- |
| Cédula/aceitação | Representação pública/privada, prova de elegibilidade e ponto de persistência |
| Unicidade | Mecanismo comum para impedir voto duplicado, inclusive alternância público/privado |
| Idempotência | Replay da mesma tentativa versus tentativa distinta do mesmo eleitor |
| Recibo | O que comprova, como verificar inclusão e quais informações revela |
| Resultado | Snapshot do eleitorado, contagens, regras/versões, quórum, maioria e encerramento verificável |

Não considerar duas constraints separadas para voto público e hash privado como garantia conjunta de um voto por pessoa. Vínculos de cédulas privadas/recibos com identidade ou reautenticação exigem análise de correlação no protocolo.

Um campo `zkp_proof`, um hash anônimo ou ocultar `user_id` não especifica sigilo. Não definir DDL eleitoral definitivo ou anunciar voto privado implementado com base nesses placeholders. Regras de empate e demais decisões eleitorais estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes).

## Invariantes e transações a verificar

1. Uma assinatura válida por cidadão/proposta, com semântica de retirada/reassinatura definida antes da feature.
2. Publicação/aceitação condicionadas a setup adotado, status, escopo territorial, autoria e quota aplicável.
3. Eleição com versão de setup e eleitorado definidos; votos aceitos não podem ser perdidos nem duplicados por replay/concorrência.
4. Aceitação e fechamento coordenados transacionalmente; resultados consistentes com registros persistidos após falha/restart.
5. Auditoria/recibo e efeito de negócio consistentes conforme protocolo, sem registrar um sucesso que não foi persistido.

Usar constraints e transações do motor real conforme a regra, com testes de rollback, concorrência, restart e falha. Índices devem apoiar consultas por status, tipo, jurisdição, autoria, proposta e data conforme uso concreto; leituras paginadas e limites são necessários. Contadores e tabelas de resultado derivados precisam de critérios de consistência e reconstrução. Implementado: `proposals.signature_count` e `ballot_counts.ballots` mudam na mesma transação da linha que alteram (sem `COUNT(*)` sob o escritor); `check-db` reporta `counter_mismatch` contra as linhas e os filtros territoriais de contas usam a coluna do tipo da jurisdição (`application::territory`).

## Auditoria e retenção

Eventos críticos incluem cadastro/login, proposta, assinatura, voto, mudança de setup, papéis e acesso de auditor. Definir campos mínimos e acesso público/restrito por tipo de evento. Não associar automaticamente conteúdo privado a user_id, IP, user-agent, horário exato ou reautenticação identificável.

O produto exige preservação auditável de votos aceitos e eventos críticos. Correções devem ser explicitadas e rastreáveis; não apagar silenciosamente histórico eleitoral. Soft delete em conteúdo administrativo/comentários não resolve retenção de dados pessoais.

Cadeia de hashes é uma técnica candidata à verificação de integridade, não uma demonstração de imutabilidade perante um operador privilegiado. Modelo de ameaça, verificação externa, custódia de chaves e política de retenção ainda precisam ser definidos. A menção de blockchain no PRD não escolhe uma implementação.

Dados sensíveis incluem e-mail, nome, endereço, biometria, credenciais/segredos TOTP, sessão, IP, user-agent e histórico privado. Definir proteção em repouso, gestão/rotação/recuperação de chaves, permissões de arquivo, exportação e backup. Não presumir que declarar AES-256 significa que o banco já os protege.

## Migrations, fixtures e operação

Toda mudança estrutural usa migration versionada, separada de fixtures. Fixtures sintéticas podem incluir Brasil/jurisdição, e-mails de teste, permissões e setup de teste, sem adotar regras de produção por seed. Não criar admin/credencial fixa de produção por seed.

Versionar/aplicar migrations de forma reproduzível, tratando execução repetida, falha parcial e compatibilidade com dados existentes. Não alterar schema manualmente nem destruir dados para fazer um teste passar.

As [bases provisórias do SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão) separam recuperação horária/backup completo diário (RPO 1h/RTO 4h), logs de 30 dias e histórico eleitoral público minimizado de dez anos; não aplicar a mesma retenção a todos os dados. Validar um procedimento consistente com o motor real e provar restauração em caminho isolado; copiar um arquivo sem validação não é evidência de recuperação.

## Ordem e evidência

Fundação: migration e dado sintético mínimo para persistência/restart. Depois identidade/sessão/RBAC e setup; propostas/assinaturas/debate; votação/recibos/resultados após resolver protocolo; notificações e consultas conforme features.

As entidades de identidade, setup, propostas, debate, votação pública demonstrativa, auditoria e notificações estão implementadas e testadas com TursoDB real (evidência na [fundação](foundation-setup.md#estado-da-evidência)). Voto privado, recibos e proteção em repouso do arquivo do banco continuam pendentes.
