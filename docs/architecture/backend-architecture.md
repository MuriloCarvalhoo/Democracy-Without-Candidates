# BACKEND-ARCHITECTURE.md

# Democracy Without Candidates
## Backend Architecture Guide

> Versão: 1.0.0  
> Status: Foundation Phase  
> Stack: NestJS + TypeORM + PostgreSQL + JWT + RBAC

---

# 1. Objetivo

Este documento define a arquitetura oficial do backend do projeto **Democracy Without Candidates**.

O objetivo é garantir que o backend seja desenvolvido com:

- Organização profissional
- Baixo acoplamento
- Alta coesão
- Segurança
- Escalabilidade
- Facilidade de manutenção
- Padrão enterprise
- Estrutura compatível com monorepo PNPM

---

# 2. Stack Backend

| Camada | Tecnologia |
|---|---|
| Framework | NestJS |
| Linguagem | TypeScript |
| ORM | TypeORM |
| Banco de Dados | PostgreSQL |
| Autenticação | JWT + Biometria + 2FA |
| Permissões | RBAC |
| Arquitetura | MVC + Monolito Modular |
| Infra Local | Docker |
| Validação | class-validator + class-transformer |
| Configuração | @nestjs/config |

---

# 3. Modelo Arquitetural

O backend seguirá o modelo:

```txt
MVC + Monolito Modular
```

## Por que monolito modular?

Nesta fase do projeto, o foco é construir uma base sólida antes de evoluir para arquiteturas mais complexas.

O monolito modular permite:

- Desenvolvimento mais rápido
- Organização por domínio
- Menor complexidade inicial
- Facilidade de testes
- Possibilidade de evolução futura para microsserviços

---

# 4. Estrutura Principal do Backend

```txt
apps/api/
│
├── src/
│   ├── main.ts
│   ├── app.module.ts
│   │
│   ├── config/
│   │   ├── app.config.ts
│   │   ├── database.config.ts
│   │   ├── auth.config.ts
│   │   └── env.validation.ts
│   │
│   ├── database/
│   │   ├── migrations/
│   │   ├── seeds/
│   │   └── data-source.ts
│   │
│   ├── common/
│   │   ├── decorators/
│   │   ├── guards/
│   │   ├── interceptors/
│   │   ├── filters/
│   │   ├── pipes/
│   │   ├── middlewares/
│   │   ├── exceptions/
│   │   ├── enums/
│   │   ├── utils/
│   │   └── constants/
│   │
│   ├── modules/
│   │   ├── auth/
│   │   ├── users/
│   │   ├── countries/
│   │   ├── jurisdictions/
│   │   ├── proposals/
│   │   ├── voting/
│   │   ├── debate/
│   │   ├── setup/
│   │   ├── audit/
│   │   ├── notifications/
│   │   ├── dashboard/
│   │   └── rbac/
│   │
│   └── shared/
│       ├── dto/
│       ├── interfaces/
│       ├── types/
│       └── abstractions/
│
├── test/
├── package.json
├── tsconfig.json
└── nest-cli.json
```

---

# 5. Responsabilidades MVC

## 5.1 Controllers

Controllers são responsáveis por receber requisições HTTP e devolver respostas.

Responsabilidades:

- Definir rotas
- Receber parâmetros
- Receber body/query/path params
- Chamar services
- Retornar respostas padronizadas

Controllers NÃO devem conter regra de negócio.

Exemplo:

```ts
@Post()
create(@Body() dto: CreateProposalDto) {
  return this.proposalsService.create(dto);
}
```

---

## 5.2 Services

Services são responsáveis pelas regras de negócio.

Responsabilidades:

- Processar regras do domínio
- Validar fluxos
- Coordenar repositories
- Aplicar regras de permissão
- Disparar eventos internos
- Orquestrar operações complexas

Exemplo:

```ts
async create(dto: CreateProposalDto) {
  await this.validateUserProposalLimit(dto.authorId);
  return this.proposalsRepository.create(dto);
}
```

---

## 5.3 Repositories

Repositories são responsáveis pela comunicação com o banco de dados.

Responsabilidades:

- Queries
- Persistência
- Busca de dados
- Atualização de entidades
- Isolamento do TypeORM

Exemplo:

```ts
async findById(id: string) {
  return this.repository.findOne({ where: { id } });
}
```

---

## 5.4 Entities

Entities representam as tabelas do banco de dados.

Responsabilidades:

- Mapeamento TypeORM
- Relacionamentos
- Colunas
- Índices
- Regras estruturais de dados

---

# 6. Estrutura Padrão de um Módulo

Todo módulo deve seguir o mesmo padrão:

```txt
modules/proposals/
│
├── controllers/
│   └── proposals.controller.ts
│
├── services/
│   └── proposals.service.ts
│
├── repositories/
│   └── proposals.repository.ts
│
├── entities/
│   └── proposal.entity.ts
│
├── dto/
│   ├── create-proposal.dto.ts
│   ├── update-proposal.dto.ts
│   └── filter-proposal.dto.ts
│
├── enums/
│   └── proposal-status.enum.ts
│
├── interfaces/
│   └── proposal.interface.ts
│
├── guards/
│
├── policies/
│
├── validators/
│
├── events/
│
└── proposals.module.ts
```

---

# 7. Módulos Prioritários

## 7.1 Auth Module

Responsável por:

- Registro
- Login
- JWT
- Refresh token
- Biometria
- 2FA
- Reautenticação para ações críticas

---

## 7.2 Users Module

Responsável por:

- Cadastro de cidadãos
- Dados pessoais
- Perfil público/privado
- Preferências
- Status da conta

---

## 7.3 Countries Module

Responsável por:

- Cadastro de países
- Países ativos/inativos
- Configurações globais por país
- Gestão inicial da jurisdição

---

## 7.4 Jurisdictions Module

Responsável por:

- País
- Estado
- Município
- Isolamento por jurisdição
- Schema do banco

---

## 7.5 RBAC Module

Responsável por:

- Papéis
- Permissões
- Guards
- Decorators
- Políticas de acesso

---

## 7.6 Proposals Module

Responsável por:

- Criação de propostas
- Rascunhos
- Publicação
- Coleta de assinaturas
- Status da proposta
- Limites por cidadão

---

## 7.7 Voting Module

Responsável por:

- Votos SIM/NÃO/ABSTENÇÃO
- Voto público
- Voto privado
- Quórum
- Maioria
- Resultado final

---

## 7.8 Debate Module

Responsável por:

- Comentários
- Threads
- Emendas
- Sugestões
- Moderação

---

## 7.9 Setup Module

Responsável por:

- Plebiscito inicial
- Configuração de parâmetros
- Meta-leis
- Alterações de setup

---

## 7.10 Audit Module

Responsável por:

- Logs críticos
- Hash de eventos
- Auditoria pública
- Auditoria avançada
- Integridade

---

# 8. RBAC — Controle de Acesso

O sistema utilizará RBAC para controlar permissões.

## Roles Oficiais

| Role | Descrição |
|---|---|
| VISITOR | Usuário não autenticado |
| PENDING_CITIZEN | Cidadão cadastrado com validação pendente |
| CITIZEN | Cidadão autenticado e validado |
| AUDITOR | Auditor independente |
| COUNTRY_ADMIN | Administrador de país/jurisdição |
| MASTER_ADMIN | Administrador global da plataforma |

---

# 9. Estratégia de Guards

Guards serão usados para proteger rotas.

Exemplos:

```txt
JwtAuthGuard
RolesGuard
BiometricReauthGuard
CountryAccessGuard
ProposalOwnerGuard
```

---

# 10. Decorators

Decorators serão usados para simplificar permissões e contexto.

Exemplos:

```ts
@Roles('CITIZEN')
@CurrentUser()
@Public()
@RequireBiometricReauth()
```

---

# 11. Autenticação

Fluxo base:

```txt
1. Usuário informa credenciais
2. Sistema valida identidade
3. Sistema solicita biometria facial
4. Sistema solicita 2FA
5. Sistema gera JWT
6. Sistema aplica RBAC
```

Ações críticas exigem reautenticação:

- Votar
- Assinar proposta
- Criar proposta
- Alterar dados sensíveis
- Alterar privacidade

---

# 12. Multi-Tenant por Schema

O backend deve operar com isolamento por schema no PostgreSQL.

Exemplo:

```sql
brasil.users
brasil.proposals
brasil.votes

portugal.users
portugal.proposals
portugal.votes
```

## Objetivo

- Separar dados por país/jurisdição
- Reduzir risco de vazamento entre países
- Facilitar backup por jurisdição
- Facilitar auditoria
- Permitir escalabilidade futura

---

# 13. TypeORM

O TypeORM será utilizado para:

- Mapear entidades
- Gerenciar migrations
- Realizar queries
- Controlar relacionamentos
- Organizar repositórios

## Regras

- Toda alteração estrutural no banco deve gerar migration
- Não alterar banco manualmente em produção
- Entities devem refletir o modelo oficial
- Repositories devem isolar queries complexas

---

# 14. DTOs e Validações

DTOs serão obrigatórios para entrada de dados.

Exemplo:

```ts
export class CreateProposalDto {
  @IsString()
  title: string;

  @IsString()
  description: string;

  @IsEnum(ProposalType)
  type: ProposalType;
}
```

Objetivos:

- Validar entrada
- Reduzir erros
- Padronizar payloads
- Proteger a API contra dados inválidos

---

# 15. Tratamento de Erros

O backend deve possuir tratamento de erro padronizado.

Estrutura recomendada:

```json
{
  "statusCode": 400,
  "message": "Invalid proposal data",
  "error": "Bad Request",
  "timestamp": "2026-05-07T00:00:00.000Z",
  "path": "/api/v1/proposals"
}
```

Componentes:

- Exception Filters
- Custom Exceptions
- Validation Pipes
- Logs estruturados

---

# 16. Segurança

Práticas obrigatórias:

- JWT seguro
- Hash de senhas
- Rate limit
- CORS configurado
- Helmet
- Validação de entrada
- Sanitização
- Logs de auditoria
- Reautenticação em ações críticas
- Separação de permissões por role
- Nunca armazenar imagem biométrica bruta

---

# 17. Versionamento da API

A API deve utilizar versionamento.

Exemplo:

```txt
/api/v1/auth/login
/api/v1/proposals
/api/v1/voting
```

Benefícios:

- Compatibilidade futura
- Evolução controlada
- Redução de breaking changes

---

# 18. Padrão de Resposta

Resposta de sucesso:

```json
{
  "success": true,
  "data": {},
  "message": "Operation completed successfully"
}
```

Resposta de erro:

```json
{
  "success": false,
  "error": {
    "message": "Unauthorized",
    "statusCode": 401
  }
}
```

---

# 19. Logs e Auditoria

Eventos críticos devem ser auditados:

- Criação de conta
- Login
- Criação de proposta
- Assinatura
- Voto
- Alteração de setup
- Alteração de permissões
- Acesso de auditor

Cada evento deve registrar:

- ID do usuário
- Tipo de evento
- Timestamp
- IP
- User-Agent
- Hash do evento
- Jurisdição

---

# 20. Testes

Tipos de testes recomendados:

| Tipo | Objetivo |
|---|---|
| Unitários | Testar services e regras |
| Integração | Testar módulos e banco |
| E2E | Testar fluxo completo da API |
| Segurança | Testar permissões e acesso |

---

# 21. Convenções de Código

## Nome de arquivos

```txt
kebab-case
```

Exemplos:

```txt
create-proposal.dto.ts
proposal-status.enum.ts
proposals.service.ts
```

## Nome de classes

```txt
PascalCase
```

Exemplos:

```txt
CreateProposalDto
ProposalsService
ProposalEntity
```

## Nome de variáveis e funções

```txt
camelCase
```

Exemplos:

```txt
createProposal()
userId
proposalStatus
```

---

# 22. Ordem Recomendada de Implementação

## Fase 1 — Base

- Configuração NestJS
- Configuração TypeORM
- Configuração PostgreSQL
- Configuração ENV
- Health check
- Estrutura comum

## Fase 2 — Core

- Users
- Auth
- JWT
- RBAC
- Countries
- Jurisdictions

## Fase 3 — Produto

- Proposals
- Signatures
- Debate
- Voting
- Results

## Fase 4 — Avançado

- Audit
- Setup
- Notifications
- Blockchain/ZKP futuro

---

# 23. O que NÃO fazer agora

Nesta fase, evitar:

- Microsserviços
- Blockchain real
- ZKP completo
- Kubernetes
- Otimização prematura
- Complexidade desnecessária
- Regras duplicadas no frontend
- Controllers com regra de negócio

---

# 24. Conclusão

A arquitetura backend do Democracy Without Candidates deve priorizar:

- Fundação sólida
- Organização modular
- Segurança
- Separação clara de responsabilidades
- Escalabilidade futura
- Simplicidade inicial
- Evolução controlada

O foco inicial deve ser construir um backend limpo, bem estruturado e preparado para crescer.

---

# Democracy Without Candidates
## Backend Architecture Guide
