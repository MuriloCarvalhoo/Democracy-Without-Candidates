# FOUNDATION-SETUP.md

# Democracy Without Candidates
## Foundation Setup Guide

> Versão: 1.0.0  
> Status: Foundation Phase

---

# 1. Objetivo

Este documento define todo o processo inicial de configuração do ambiente de desenvolvimento do projeto Democracy Without Candidates.

O objetivo é garantir:

- Padronização do ambiente
- Facilidade de onboarding
- Inicialização rápida do projeto
- Compatibilidade entre desenvolvedores
- Estrutura consistente do monorepo

---

# 2. Pré-requisitos

Antes de iniciar o projeto, é necessário possuir instalado:

| Ferramenta | Versão Recomendada |
|---|---|
| Node.js | 24.x ou 26.x |
| PNPM | Última versão |
| Docker | Última versão |
| Docker Compose | Última versão |
| Git | Última versão |

---

# 3. Instalação do PNPM

Instalar globalmente:

```bash
npm install -g pnpm
```

Verificar instalação:

```bash
pnpm -v
```

---

# 4. Estrutura Inicial do Projeto

Estrutura base do monorepo:

```txt
Democracy-Without-Candidates/
│
├── apps/
│   ├── api/
│   └── web/
│
├── packages/
│
├── infra/
│
├── docs/
│
├── docker-compose.yml
├── package.json
├── pnpm-workspace.yaml
└── tsconfig.base.json
```

---

# 5. Inicialização do Monorepo

## Criar package.json raiz

```bash
pnpm init
```

---

## Criar pnpm-workspace.yaml

```yaml
packages:
  - 'apps/*'
  - 'packages/*'
```

---

# 6. Inicialização do Backend (NestJS)

## Criar aplicação backend

```bash
pnpm create nest apps/api
```

---

## Instalar dependências principais

```bash
pnpm add @nestjs/typeorm typeorm pg
```

---

## Dependências recomendadas

```bash
pnpm add class-validator class-transformer
pnpm add @nestjs/config
pnpm add @nestjs/jwt passport passport-jwt
pnpm add bcrypt
```

---

# 7. Inicialização do Frontend (React PWA)

## Criar aplicação React

```bash
pnpm create vite apps/web --template react-ts
```

---

## Instalar dependências base

```bash
cd apps/web

pnpm add react-router-dom
pnpm add axios
pnpm add zustand
pnpm add react-hook-form
```

---

## Configurar PWA

```bash
pnpm add vite-plugin-pwa
```

---

# 8. Configuração Docker

Estrutura recomendada:

```txt
infra/docker/
│
├── api/
├── web/
├── postgres/
└── nginx/
```

---

# 9. Docker Compose

Serviços iniciais:

| Serviço | Porta |
|---|---|
| React Web | 3000 |
| NestJS API | 3333 |
| PostgreSQL | 5432 |
| PgAdmin | 5050 |

---

# 10. Exemplo docker-compose.yml

```yaml
version: '3.9'

services:
  api:
    build:
      context: .
      dockerfile: infra/docker/api/Dockerfile
    ports:
      - '3333:3333'
    depends_on:
      - postgres

  web:
    build:
      context: .
      dockerfile: infra/docker/web/Dockerfile
    ports:
      - '3000:3000'

  postgres:
    image: postgres:16
    restart: always
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: democracy
    ports:
      - '5432:5432'

  pgadmin:
    image: dpage/pgadmin4
    restart: always
    environment:
      PGADMIN_DEFAULT_EMAIL: admin@admin.com
      PGADMIN_DEFAULT_PASSWORD: admin
    ports:
      - '5050:80'
```

---

# 11. Configuração TypeScript Base

## tsconfig.base.json

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "CommonJS",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "baseUrl": "."
  }
}
```

---

# 12. Configuração ESLint

Objetivos:

- Padronização de código
- Organização
- Qualidade
- Prevenção de erros

Recomendado:

- ESLint
- Prettier
- Husky
- Lint-Staged

---

# 13. Estrutura de ENV

## Arquivo .env.example

```env
NODE_ENV=development

API_PORT=3333
WEB_PORT=3000

DATABASE_HOST=localhost
DATABASE_PORT=5432
DATABASE_USER=postgres
DATABASE_PASSWORD=postgres
DATABASE_NAME=democracy

JWT_SECRET=change_this_secret
```

---

# 14. Ordem Recomendada de Configuração

## Foundation Phase

### Etapa 1
- Criar monorepo
- Configurar PNPM

### Etapa 2
- Criar backend NestJS
- Criar frontend React

### Etapa 3
- Configurar Docker
- Configurar PostgreSQL

### Etapa 4
- Configurar TypeScript
- Configurar ESLint

### Etapa 5
- Estruturar arquitetura MVC
- Criar módulos principais

---

# 15. Módulos Prioritários

Primeiros módulos do backend:

- auth
- users
- countries
- jurisdictions
- rbac

---

# 16. Ordem Recomendada de Desenvolvimento

## Foundation
- Infra
- Docker
- Banco
- Configuração base

## Core
- Auth
- JWT
- RBAC
- Usuários

## Business
- Proposals
- Voting
- Debate

## Advanced
- Auditoria
- Blockchain
- ZKP

---

# 17. Objetivo da Foundation Phase

A Foundation Phase existe para garantir:

- Estrutura sólida
- Arquitetura limpa
- Escalabilidade futura
- Segurança organizacional
- Facilidade de manutenção

Antes do desenvolvimento avançado do sistema.

---

# Democracy Without Candidates
## Foundation Setup Guide
