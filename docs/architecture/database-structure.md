# DATABASE-STRUCTURE.md

# Democracy Without Candidates
## Database Structure Guide

> Versão: 1.0.0  
> Status: Foundation Phase  
> Stack: PostgreSQL + TypeORM + Multi-Tenant Schema Strategy

---

# 1. Objetivo

Este documento define a estrutura inicial do banco de dados do projeto **Democracy Without Candidates**.

O objetivo é garantir que a base de dados seja:

- Segura
- Auditável
- Escalável
- Organizada
- Preparada para múltiplos países
- Preparada para múltiplas jurisdições
- Compatível com o backend NestJS + TypeORM
- Estruturada para votação, propostas, autenticação, RBAC e auditoria

---

# 2. Banco de Dados Oficial

| Item | Definição |
|---|---|
| Banco | PostgreSQL |
| ORM | TypeORM |
| Estratégia | Multi-tenant por schema |
| IDs | UUID |
| Migrations | Obrigatórias |
| Auditoria | Registro de eventos críticos |
| Soft Delete | Aplicável em entidades não críticas |
| Imutabilidade | Obrigatória para votos e logs críticos |

---

# 3. Estratégia Multi-Tenant

O sistema utilizará isolamento por **schema PostgreSQL**.

Cada país ou jurisdição principal poderá possuir seu próprio schema.

Exemplo:

```sql
brasil.users
brasil.proposals
brasil.votes

portugal.users
portugal.proposals
portugal.votes

argentina.users
argentina.proposals
argentina.votes
```

---

# 4. Objetivo do Isolamento por Schema

A estratégia por schema permite:

- Separar dados por país
- Reduzir risco de vazamento entre jurisdições
- Facilitar auditoria
- Facilitar backup por país
- Facilitar futuras migrações
- Melhorar organização lógica
- Permitir expansão futura para banco separado por país

---

# 5. Schema Global

Além dos schemas por país, existirá um schema global para dados da plataforma.

Exemplo:

```sql
public.countries
public.platform_admins
public.global_auditors
public.system_health
```

O schema global será usado para:

- Cadastro de países
- Administração master
- Gestão de schemas
- Configurações globais
- Saúde da plataforma

---

# 6. Convenções Gerais

## 6.1 Nome de tabelas

Usar nomes no plural e em snake_case.

Exemplos:

```txt
users
proposals
votes
audit_logs
jurisdictions
```

---

## 6.2 Nome de colunas

Usar snake_case.

Exemplos:

```txt
created_at
updated_at
deleted_at
user_id
proposal_id
jurisdiction_id
```

---

## 6.3 IDs

Todos os registros principais devem utilizar UUID.

Exemplo:

```sql
id UUID PRIMARY KEY DEFAULT gen_random_uuid()
```

---

# 7. Campos Padrão

Entidades principais devem possuir:

```txt
id
created_at
updated_at
deleted_at
```

Exemplo:

```sql
id UUID PRIMARY KEY
created_at TIMESTAMP NOT NULL
updated_at TIMESTAMP NOT NULL
deleted_at TIMESTAMP NULL
```

---

# 8. Soft Delete

Soft delete será utilizado em entidades administrativas e de conteúdo.

Aplicável em:

- users
- proposals
- comments
- notifications
- countries
- jurisdictions

Não aplicável em:

- votes
- audit_logs
- critical_events
- setup_votes

Votos e logs críticos devem ser imutáveis.

---

# 9. Entidades Globais

## 9.1 countries

Tabela responsável pelos países cadastrados na plataforma.

Campos principais:

```txt
id
name
code
default_language
status
schema_name
created_at
updated_at
deleted_at
```

Exemplo:

```txt
Brasil | BR | portugues | active | brasil
Portugal | PT | portugues | active | portugal
```

---

## 9.2 platform_admins

Tabela responsável pelos administradores globais.

Campos principais:

```txt
id
name
email
password_hash
role
status
created_at
updated_at
deleted_at
```

---

## 9.3 global_auditors

Tabela responsável pelos auditores independentes com acesso global ou parcial.

Campos principais:

```txt
id
name
email
document
status
scope
created_at
updated_at
deleted_at
```

---

# 10. Entidades por Jurisdição

Cada schema de país terá suas próprias tabelas.

Exemplo:

```txt
brasil.users
brasil.proposals
brasil.votes
brasil.audit_logs
```

---

# 11. Users

Tabela responsável pelos cidadãos.

## users

Campos principais:

```txt
id
full_name
email
document_number
birth_date
biometric_hash
password_hash
two_factor_enabled
two_factor_secret
profile_visibility
default_vote_visibility
status
jurisdiction_id
created_at
updated_at
deleted_at
```

## Regras

- document_number deve ser único por jurisdição
- biometric_hash deve ser único por jurisdição
- email deve ser único por jurisdição
- imagem biométrica bruta nunca deve ser armazenada
- apenas hash biométrico deve ser persistido

---

# 12. User Profiles

## user_profiles

Campos principais:

```txt
id
user_id
display_name
photo_url
bio
is_public
created_at
updated_at
deleted_at
```

## Objetivo

Separar dados sensíveis da conta de dados públicos do perfil.

---

# 13. Jurisdictions

## jurisdictions

Tabela responsável pela organização territorial.

Campos principais:

```txt
id
name
type
parent_id
country_id
status
created_at
updated_at
deleted_at
```

## Tipos de jurisdição

```txt
national
state
municipal
```

Exemplo:

```txt
Brasil
Rio de Janeiro
Rio de Janeiro - Município
```

---

# 14. RBAC

## roles

Campos principais:

```txt
id
name
description
created_at
updated_at
```

Roles oficiais:

```txt
VISITOR
PENDING_CITIZEN
CITIZEN
AUDITOR
COUNTRY_ADMIN
MASTER_ADMIN
```

---

## permissions

Campos principais:

```txt
id
name
description
resource
action
created_at
updated_at
```

Exemplos:

```txt
proposal:create
proposal:sign
proposal:vote
audit:read
country:manage
```

---

## user_roles

Campos principais:

```txt
id
user_id
role_id
created_at
updated_at
```

---

## role_permissions

Campos principais:

```txt
id
role_id
permission_id
created_at
updated_at
```

---

# 15. Proposals

## proposals

Tabela responsável pelas propostas.

Campos principais:

```txt
id
title
summary
content
type
status
author_id
jurisdiction_id
signature_goal
signature_count
debate_starts_at
debate_ends_at
voting_starts_at
voting_ends_at
created_at
updated_at
deleted_at
```

## Tipos

```txt
normal_law
urgent_law
setup_change
initial_setup
```

## Status

```txt
draft
collecting_signatures
in_debate
in_voting
approved
rejected
quorum_not_reached
expired
archived
```

---

# 16. Proposal Signatures

## proposal_signatures

Tabela responsável pelas assinaturas de apoio.

Campos principais:

```txt
id
proposal_id
user_id
jurisdiction_id
signed_at
biometric_reauth_id
created_at
```

## Regras

- Um cidadão só pode assinar uma proposta uma vez
- Assinatura exige reautenticação biométrica
- Assinaturas devem ser auditadas

Índice único recomendado:

```sql
UNIQUE (proposal_id, user_id)
```

---

# 17. Debate

## comments

Campos principais:

```txt
id
proposal_id
user_id
parent_id
content
status
created_at
updated_at
deleted_at
```

## Regras

- Comentários podem ser encadeados
- Comentários podem ser removidos por moderação
- Histórico crítico deve permanecer auditável

---

## amendments

Tabela responsável por emendas e sugestões de alteração.

Campos principais:

```txt
id
proposal_id
user_id
title
content
status
created_at
updated_at
deleted_at
```

---

# 18. Voting

## votes

Tabela responsável pelos votos.

Campos principais:

```txt
id
proposal_id
jurisdiction_id
vote_value
vote_visibility
public_user_id
anonymous_vote_hash
zkp_proof
biometric_reauth_id
created_at
```

## Valores de voto

```txt
yes
no
abstention
```

## Visibilidade

```txt
public
private
```

## Regras críticas

- Um cidadão só pode votar uma vez por proposta
- Voto não pode ser alterado
- Voto não pode ser deletado
- Voto privado não deve expor user_id diretamente
- Voto público pode possuir vínculo com public_user_id
- Voto privado deve usar hash anônimo ou prova criptográfica
- Reautenticação biométrica obrigatória

---

# 19. Vote Receipts

## vote_receipts

Tabela responsável por comprovantes de voto sem expor o conteúdo privado.

Campos principais:

```txt
id
proposal_id
user_id
receipt_hash
created_at
```

## Objetivo

Permitir ao cidadão verificar que seu voto foi computado sem comprometer a privacidade.

---

# 20. Voting Results

## voting_results

Tabela responsável pelo resultado consolidado da votação.

Campos principais:

```txt
id
proposal_id
total_eligible_voters
total_votes
yes_votes
no_votes
abstention_votes
quorum_required
quorum_reached
majority_required
result
calculated_at
created_at
```

## Resultados possíveis

```txt
approved
rejected
quorum_not_reached
tie
```

---

# 21. Setup

## setup_configurations

Tabela responsável pelas configurações da jurisdição.

Campos principais:

```txt
id
jurisdiction_id
normal_signature_percentage
urgent_signature_percentage
normal_quorum_percentage
urgent_quorum_percentage
initial_setup_quorum_percentage
setup_change_quorum_percentage
setup_change_majority_percentage
normal_collection_days
normal_debate_days
normal_voting_days
urgent_collection_days
urgent_voting_days
normal_proposal_limit_per_year
urgent_proposal_limit_per_year
status
created_at
updated_at
```

---

## setup_history

Tabela responsável pelo histórico de alterações das regras.

Campos principais:

```txt
id
setup_configuration_id
proposal_id
changed_by_proposal_id
old_value
new_value
changed_at
created_at
```

---

# 22. Notifications

## notifications

Campos principais:

```txt
id
user_id
type
title
message
read_at
created_at
deleted_at
```

Tipos:

```txt
proposal_created
voting_opened
deadline_reminder
proposal_approved
proposal_rejected
```

---

# 23. Audit Logs

## audit_logs

Tabela responsável pela auditoria de eventos críticos.

Campos principais:

```txt
id
event_type
user_id
proposal_id
jurisdiction_id
metadata
ip_address
user_agent
event_hash
previous_hash
created_at
```

## Eventos auditáveis

```txt
user_created
user_login
proposal_created
proposal_signed
vote_cast
setup_changed
role_changed
auditor_accessed
```

## Regras

- Logs não podem ser apagados
- Logs devem formar cadeia de hash
- Logs devem permitir verificação de integridade
- Logs não devem expor voto privado

---

# 24. Biometric Reauthentication

## biometric_reauthentications

Tabela responsável pelos registros de reautenticação.

Campos principais:

```txt
id
user_id
action_type
success
provider
created_at
expires_at
```

Ações críticas:

```txt
vote
sign_proposal
create_proposal
update_sensitive_profile
change_privacy
```

---

# 25. Índices Recomendados

## users

```sql
CREATE INDEX idx_users_document_number ON users(document_number);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_jurisdiction_id ON users(jurisdiction_id);
```

---

## proposals

```sql
CREATE INDEX idx_proposals_status ON proposals(status);
CREATE INDEX idx_proposals_type ON proposals(type);
CREATE INDEX idx_proposals_jurisdiction_id ON proposals(jurisdiction_id);
CREATE INDEX idx_proposals_author_id ON proposals(author_id);
```

---

## votes

```sql
CREATE INDEX idx_votes_proposal_id ON votes(proposal_id);
CREATE INDEX idx_votes_jurisdiction_id ON votes(jurisdiction_id);
CREATE INDEX idx_votes_created_at ON votes(created_at);
```

---

## audit_logs

```sql
CREATE INDEX idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX idx_audit_logs_jurisdiction_id ON audit_logs(jurisdiction_id);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
```

---

# 26. Constraints Importantes

## Um voto por cidadão por proposta

Para votos públicos:

```sql
UNIQUE (proposal_id, public_user_id)
```

Para votos privados, usar hash anônimo único:

```sql
UNIQUE (proposal_id, anonymous_vote_hash)
```

---

## Uma assinatura por cidadão por proposta

```sql
UNIQUE (proposal_id, user_id)
```

---

## Documento único por jurisdição

```sql
UNIQUE (document_number, jurisdiction_id)
```

---

# 27. Estratégia de Migrations

Regras:

- Toda alteração estrutural deve ser feita via migration
- Nunca alterar banco manualmente
- Migrations devem ser versionadas no Git
- Seeds devem ser separadas de migrations
- Cada módulo pode ter suas próprias migrations

Estrutura:

```txt
database/
├── migrations/
├── seeds/
└── data-source.ts
```

---

# 28. Seeds Iniciais

Seeds recomendadas:

- Roles padrão
- Permissões padrão
- País inicial de teste
- Jurisdição inicial
- Admin master
- Configuração inicial padrão

---

# 29. Auditoria e Imutabilidade

Entidades imutáveis:

- votes
- audit_logs
- vote_receipts
- setup_history

Essas tabelas não devem permitir:

- update
- delete
- soft delete

Alterações devem ser registradas por eventos adicionais.

---

# 30. Estratégia de Performance

Práticas recomendadas:

- Índices em colunas de busca
- Paginação obrigatória
- Filtros por jurisdição
- Evitar consultas sem limit
- Separar leitura pública de dados sensíveis
- Cache futuro para estatísticas
- Agregação de resultados de votação em tabela própria

---

# 31. Segurança de Dados

Regras obrigatórias:

- Nunca armazenar imagem biométrica bruta
- Criptografar dados sensíveis
- Separar perfil público de dados pessoais
- Não expor user_id em votos privados
- Auditar ações críticas
- Validar jurisdição em todas as queries
- Usar least privilege para usuários do banco

---

# 32. Dados Sensíveis

Dados considerados sensíveis:

- Documento
- Biometria
- 2FA secret
- Dados pessoais
- IP
- User-Agent
- Histórico de autenticação
- Voto privado

Esses dados devem ter tratamento especial de segurança.

---

# 33. Relacionamentos Principais

```txt
Country 1:N Jurisdictions
Jurisdiction 1:N Users
Jurisdiction 1:N Proposals
User 1:N Proposals
Proposal 1:N Signatures
Proposal 1:N Votes
Proposal 1:N Comments
Proposal 1:1 VotingResult
User N:N Roles
Role N:N Permissions
```

---

# 34. Ordem Recomendada de Implementação

## Fase 1 — Foundation

- countries
- jurisdictions
- users
- roles
- permissions
- user_roles
- role_permissions

---

## Fase 2 — Core

- auth
- biometric_reauthentications
- proposals
- proposal_signatures

---

## Fase 3 — Voting

- votes
- vote_receipts
- voting_results

---

## Fase 4 — Advanced

- setup_configurations
- setup_history
- audit_logs
- notifications
- comments
- amendments

---

# 35. O que NÃO fazer agora

Evitar nesta fase:

- Banco separado por país
- Microsserviços de banco
- Blockchain real imediato
- ZKP completo imediato
- Triggers complexas sem necessidade
- Otimização prematura
- Duplicação de tabelas sem justificativa
- Dados sensíveis sem criptografia

---

# 36. Conclusão

A estrutura de banco do Democracy Without Candidates deve priorizar:

- Integridade
- Segurança
- Isolamento por jurisdição
- Auditoria
- Imutabilidade de votos
- Escalabilidade
- Clareza de relacionamento
- Evolução gradual

O banco é uma das partes mais críticas do projeto, pois sustenta autenticação, votação, propostas, auditoria, transparência e confiança pública.

---

# Democracy Without Candidates
## Database Structure Guide
