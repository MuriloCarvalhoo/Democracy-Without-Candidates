# REPOSITORY-STRUCTURE.md

# Democracy Without Candidates
## Enterprise Repository Structure & Architecture Plan

> Versão: 1.0.0
> Status: Foundation Architecture
> Stack: NestJS + React PWA + PostgreSQL + PNPM Monorepo + Docker

---

# 1. Objetivo

Este documento define:

- Estrutura oficial do monorepo
- Arquitetura backend (NestJS MVC)
- Organização frontend React PWA
- Estratégia Docker
- Convenções de projeto
- Organização compartilhada de pacotes
- Padrões de escalabilidade
- Base estrutural para desenvolvimento enterprise

O objetivo é garantir:

- Escalabilidade
- Organização
- Baixo acoplamento
- Facilidade de manutenção
- Separação clara de responsabilidades
- Crescimento sustentável do projeto

---

# 2. Stack Oficial

| Camada | Tecnologia |
|---|---|
| Monorepo | PNPM Workspaces |
| Runtime | Node.js 24/26 |
| Backend | NestJS + TypeORM |
| Frontend | React + Vite + PWA |
| Banco de Dados | PostgreSQL |
| Infra Local | Docker + Docker Compose |
| Autenticação | Face Recognition + TOTP |
| Permissões | RBAC |
| Linguagem | TypeScript |
| ORM | TypeORM |
| API | REST API |
| Segurança | JWT + TLS + AES |

---

# 3. Estrutura Oficial do Monorepo

```txt
Democracy-Without-Candidates/
│
├── apps/
│   ├── api/
│   └── web/
│
├── packages/
│   ├── ui/
│   ├── shared/
│   ├── eslint-config/
│   ├── typescript-config/
│   └── constants/
│
├── infra/
│   ├── docker/
│   ├── postgres/
│   └── nginx/
│
├── docs/
│   ├── architecture/
│   ├── api/
│   ├── wireframes/
│   ├── database/
│   └── security/
│
├── scripts/
│
├── .github/
│   └── workflows/
│
├── docker-compose.yml
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.base.json
├── .gitignore
├── .editorconfig
├── .env.example
├── README.md
└── LICENSE
```

---

# 4. Estrutura do Backend (NestJS)

## 4.1 Arquitetura Base

O backend seguirá:

- Arquitetura MVC
- Monolito Modular
- Separação por domínio
- Baixo acoplamento
- Alta coesão

Inicialmente NÃO serão utilizados microsserviços.

O projeto seguirá arquitetura modular enterprise.

---

# 5. Estrutura Oficial do Backend

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
│   │   ├── middlewares/
│   │   ├── pipes/
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
│   │   ├── notifications/
│   │   ├── setup/
│   │   ├── audit/
│   │   ├── rbac/
│   │   └── dashboard/
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

# 6. Estrutura Interna de Módulos (Padrão)

Todos os módulos seguirão a mesma estrutura.

Exemplo:

```txt
modules/proposals/
│
├── controllers/
├── services/
├── repositories/
├── entities/
├── dto/
├── interfaces/
├── guards/
├── policies/
├── validators/
├── events/
├── proposals.module.ts
└── proposals.controller.ts
```

---

# 7. Responsabilidades MVC

## Controllers

Responsáveis por:

- Receber requisições
- Validar entrada
- Retornar resposta
- Delegar regras ao service

Controllers NÃO devem conter regra de negócio.

---

## Services

Responsáveis por:

- Regras de negócio
- Fluxos do sistema
- Validações complexas
- Integrações
- Permissões

---

## Repositories

Responsáveis por:

- Comunicação com banco
- Queries
- Persistência
- Isolamento ORM

---

## Entities

Responsáveis por:

- Estrutura de dados
- Mapeamento TypeORM

---

# 8. Estratégia Multi-Tenant

O sistema utilizará:

## Schema por jurisdição

Exemplo:

```sql
brasil.users
brasil.proposals

argentina.users
argentina.proposals
```

Benefícios:

- Isolamento forte
- Facilidade de backup
- Escalabilidade futura
- Segurança organizacional
- Menor risco de vazamento entre países

---

# 9. Estrutura do Frontend (React PWA)

## Objetivos

O frontend será:

- Mobile-first
- Responsivo
- Instalável como PWA
- Escalável
- Modular
- Reutilizável

---

# 10. Estrutura Oficial do Frontend

```txt
apps/web/
│
├── public/
│
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   │
│   ├── app/
│   │   ├── providers/
│   │   ├── router/
│   │   ├── layouts/
│   │   └── store/
│   │
│   ├── pages/
│   │   ├── auth/
│   │   ├── dashboard/
│   │   ├── proposals/
│   │   ├── voting/
│   │   ├── setup/
│   │   ├── profile/
│   │   ├── audit/
│   │   └── admin/
│   │
│   ├── components/
│   │   ├── ui/
│   │   ├── forms/
│   │   ├── layout/
│   │   ├── feedback/
│   │   └── charts/
│   │
│   ├── features/
│   │   ├── auth/
│   │   ├── proposals/
│   │   ├── voting/
│   │   ├── debate/
│   │   └── notifications/
│   │
│   ├── services/
│   │   ├── api/
│   │   ├── auth/
│   │   └── storage/
│   │
│   ├── hooks/
│   ├── contexts/
│   ├── types/
│   ├── utils/
│   ├── constants/
│   ├── assets/
│   ├── styles/
│   └── i18n/
│
├── vite.config.ts
├── tsconfig.json
└── package.json
```

---

# 11. Organização dos Packages Compartilhados

## packages/ui

Biblioteca de componentes reutilizáveis.

Exemplos:

- Buttons
- Inputs
- Modals
- Tables
- Cards
- Dialogs
- Layouts

---

## packages/shared

Código compartilhado entre frontend e backend.

Exemplos:

- DTOs
- Types
- Interfaces
- Helpers
- Validators
- Enums

---

## packages/constants

Constantes globais.

Exemplos:

- Roles
- Status
- Permissions
- Limites
- Configurações default

---

# 12. Estratégia Docker

## Objetivos

Docker será utilizado para:

- Desenvolvimento local
- Padronização de ambiente
- Facilidade de onboarding
- Isolamento de serviços

---

# 13. Estrutura Docker

```txt
infra/docker/
│
├── api/
│   └── Dockerfile
│
├── web/
│   └── Dockerfile
│
├── postgres/
│   └── Dockerfile
│
└── nginx/
    └── Dockerfile
```

---

# 14. Estrutura do Docker Compose

Serviços iniciais:

| Serviço | Porta |
|---|---|
| Frontend React | 3000 |
| Backend NestJS | 3333 |
| PostgreSQL | 5432 |
| PgAdmin | 5050 |

---

# 15. Exemplo docker-compose.yml

```yaml
version: '3.9'

services:
  api:
    build:
      context: .
      dockerfile: infra/docker/api/Dockerfile
    ports:
      - '3333:3333'
    env_file:
      - .env
    depends_on:
      - postgres

  web:
    build:
      context: .
      dockerfile: infra/docker/web/Dockerfile
    ports:
      - '3000:3000'
    depends_on:
      - api

  postgres:
    image: postgres:16
    restart: always
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: democracy
    ports:
      - '5432:5432'
    volumes:
      - postgres_data:/var/lib/postgresql/data

  pgadmin:
    image: dpage/pgadmin4
    restart: always
    environment:
      PGADMIN_DEFAULT_EMAIL: admin@admin.com
      PGADMIN_DEFAULT_PASSWORD: admin
    ports:
      - '5050:80'

volumes:
  postgres_data:
```

---

# 16. Configurações Raiz

## pnpm-workspace.yaml

```yaml
packages:
  - 'apps/*'
  - 'packages/*'
```

---

## tsconfig.base.json

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "CommonJS",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "baseUrl": "."
  }
}
```

---

# 17. Convenções de Código

## Backend

| Regra | Convenção |
|---|---|
| Arquivos | kebab-case |
| Classes | PascalCase |
| Variáveis | camelCase |
| Interfaces | IExample |
| DTOs | CreateProposalDto |
| Services | ProposalService |
| Controllers | ProposalController |

---

## Frontend

| Regra | Convenção |
|---|---|
| Componentes | PascalCase |
| Hooks | useExample |
| Pastas | kebab-case |
| Contexts | ExampleContext |
| Providers | ExampleProvider |

---

# 18. Estratégia de Segurança

## Backend

- JWT
- Refresh Token
- Guards
- Rate Limit
- Helmet
- CORS
- Criptografia AES
- TLS 1.3

---

## Frontend

- CSP
- Secure Storage
- Sanitização
- Proteção XSS
- Proteção CSRF

---

# 19. Estratégia RBAC

## Roles Oficiais

| Role |
|---|
| VISITOR |
| PENDING_CITIZEN |
| CITIZEN |
| AUDITOR |
| COUNTRY_ADMIN |
| MASTER_ADMIN |

---

# 20. Estratégia de Escalabilidade

Inicialmente:

- Monolito modular
- PostgreSQL único
- Docker Compose
- REST API

Futuro:

- Kubernetes
- Redis
- Queue System
- CDN
- Microsserviços
- Event Bus
- Observabilidade avançada

---

# 21. Estratégia Git

## Branches

| Branch | Objetivo |
|---|---|
| main | Produção |
| develop | Desenvolvimento |
| feature/* | Novas funcionalidades |
| hotfix/* | Correções urgentes |

---

# 22. Estrutura de Commits

Padrão Conventional Commits.

Exemplos:

```bash
feat(auth): add facial authentication
fix(voting): correct quorum validation
refactor(proposals): improve proposal flow
```

---

# 23. Ordem Recomendada de Desenvolvimento

## Fase 1 — Foundation

- Monorepo
- Docker
- PostgreSQL
- NestJS
- React
- ESLint
- TypeScript
- Configuração base

---

## Fase 2 — Core

- Auth
- JWT
- RBAC
- Users
- Countries
- Jurisdictions

---

## Fase 3 — Business

- Proposals
- Voting
- Debate
- Setup
- Dashboard

---

## Fase 4 — Advanced

- Auditoria
- Logs
- Blockchain
- ZKP
- Observabilidade

---

# 24. Conclusão

Esta estrutura foi planejada para:

- Crescimento enterprise
- Escalabilidade futura
- Manutenção simplificada
- Segurança organizacional
- Organização profissional
- Evolução sustentável do produto

O foco inicial será:

- Fundação sólida
- Arquitetura limpa
- Modularização
- Segurança
- Padronização

Antes de otimizações avançadas ou arquitetura distribuída.

---

# Democracy Without Candidates
## Repository Structure Plan — Enterprise Foundation

