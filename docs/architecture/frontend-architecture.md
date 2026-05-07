# FRONTEND-ARCHITECTURE.md

# Democracy Without Candidates
## Frontend Architecture Guide

> Versão: 1.0.0  
> Status: Foundation Phase  
> Stack: React + Vite + TypeScript + PWA

---

# 1. Objetivo

Este documento define a arquitetura oficial do frontend do projeto Democracy Without Candidates.

O objetivo é garantir:

- Organização escalável
- Estrutura modular
- Reutilização de componentes
- Facilidade de manutenção
- Responsividade
- Compatibilidade mobile-first
- Padronização visual e técnica
- Base sólida para crescimento futuro

---

# 2. Stack Frontend

| Camada | Tecnologia |
|---|---|
| Framework | React |
| Build Tool | Vite |
| Linguagem | TypeScript |
| Navegação | React Router DOM |
| Gerenciamento de Estado | Zustand |
| Requisições HTTP | Axios |
| Formulários | React Hook Form |
| PWA | vite-plugin-pwa |
| Estilização | CSS Modules / Tailwind (definição futura) |
| Validação | Zod (futuro recomendado) |

---

# 3. Filosofia da Arquitetura Frontend

O frontend será desenvolvido seguindo os princípios:

- Mobile-first
- Modularização
- Componentização
- Separação de responsabilidades
- Reutilização
- Escalabilidade
- Acessibilidade
- Performance

A arquitetura deve permitir que o projeto cresça sem se tornar desorganizado.

---

# 4. Estrutura Oficial do Frontend

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

# 5. Organização por Camadas

## 5.1 Pages

A pasta pages representa as telas principais da aplicação.

Exemplos:

```txt
/pages/auth/login
/pages/proposals
/pages/dashboard
/pages/profile
```

Cada página deve representar uma rota principal do sistema.

---

## 5.2 Components

A pasta components contém componentes reutilizáveis.

Objetivo:

- Reutilização
- Padronização visual
- Redução de duplicação

Exemplos:

```txt
Button
Input
Modal
Card
Badge
Table
Dialog
Loader
Toast
```

---

## 5.3 Features

Features representam regras de negócio específicas do frontend.

Exemplos:

```txt
auth
proposals
voting
debate
notifications
```

Cada feature pode conter:

- hooks
- services
- components
- store
- utils

---

## 5.4 Services

Services centralizam comunicação externa.

Exemplos:

```txt
api/
auth/
storage/
```

Responsabilidades:

- Comunicação HTTP
- Tokens
- Sessão
- Persistência local
- Configuração Axios

---

## 5.5 Hooks

Hooks encapsulam lógica reutilizável.

Exemplos:

```txt
useAuth
useVoting
useProposal
useDebounce
usePermissions
```

---

## 5.6 Contexts e Store

Responsáveis por gerenciamento global de estado.

Exemplos:

- autenticação
- tema
- sessão
- notificações
- preferências

Inicialmente será utilizado:

```txt
Zustand
```

---

# 6. Organização de Rotas

As rotas devem ser organizadas por domínio.

Exemplo:

```txt
/auth/login
/dashboard
/proposals
/proposals/:id
/proposals/:id/vote
/profile
/audit
/admin
```

---

# 7. Layouts

Layouts definem estruturas visuais reutilizáveis.

Exemplos:

```txt
AuthLayout
DashboardLayout
AdminLayout
PublicLayout
```

Objetivos:

- Padronizar navegação
- Reutilizar estrutura visual
- Facilitar manutenção

---

# 8. Estratégia Mobile-First

Todo o frontend será desenvolvido priorizando dispositivos móveis.

Objetivos:

- Melhor experiência mobile
- Responsividade
- Compatibilidade PWA
- Acessibilidade
- Performance

---

# 9. PWA (Progressive Web App)

O sistema será instalável como aplicativo.

Objetivos:

- Instalação no celular
- Instalação no desktop
- Offline parcial
- Melhor experiência mobile
- Push notifications futuras

Tecnologia:

```txt
vite-plugin-pwa
```

---

# 10. Estados Globais de Interface

Todas as telas devem possuir estados padronizados.

## Estados obrigatórios

| Estado | Objetivo |
|---|---|
| Loading | Carregamento |
| Empty | Nenhum dado |
| Error | Erro de operação |
| Success | Operação concluída |

---

# 11. Feedback Visual

O sistema deve possuir feedback constante para o usuário.

Exemplos:

- Toasts
- Loaders
- Skeletons
- Alertas
- Mensagens de erro
- Confirmações

---

# 12. Acessibilidade

O frontend deve seguir padrões WCAG 2.1 AA.

Objetivos:

- Navegação por teclado
- Leitores de tela
- Contraste adequado
- Labels acessíveis
- Semântica HTML correta

---

# 13. Internacionalização (i18n)

O frontend será preparado para múltiplos idiomas.

Estrutura recomendada:

```txt
src/i18n/
```

Idiomas previstos:

- Português
- Inglês
- Espanhol

---

# 14. Estratégia de Consumo da API

Toda comunicação backend deve passar pela camada services/api.

Objetivos:

- Centralização
- Facilidade de manutenção
- Interceptors
- Controle de autenticação
- Tratamento de erros

---

# 15. Axios Instance

Recomendado criar uma instância centralizada:

```ts
api.ts
```

Responsabilidades:

- Base URL
- JWT
- Interceptors
- Refresh token
- Tratamento global de erro

---

# 16. Estrutura de Componentes

## Componentes UI

Componentes genéricos.

Exemplos:

```txt
Button
Input
Card
Badge
Modal
Tooltip
```

---

## Componentes de Layout

Estrutura visual.

Exemplos:

```txt
Sidebar
Header
Navbar
Footer
Container
```

---

## Componentes de Feedback

Estados e notificações.

Exemplos:

```txt
Loader
Toast
EmptyState
ErrorState
Skeleton
```

---

# 17. Estratégia de Segurança Frontend

Práticas obrigatórias:

- Sanitização
- Proteção XSS
- CSP
- Armazenamento seguro
- Nunca armazenar dados sensíveis inseguros
- Controle de permissões por rota
- Reautenticação para ações críticas

---

# 18. Estrutura de Assets

```txt
assets/
│
├── icons/
├── images/
├── logos/
├── illustrations/
└── fonts/
```

---

# 19. Estrutura de Estilos

```txt
styles/
│
├── globals/
├── themes/
├── variables/
├── animations/
└── utilities/
```

---

# 20. Convenções de Código

## Componentes

```txt
PascalCase
```

Exemplos:

```txt
ProposalCard.tsx
VoteModal.tsx
DashboardLayout.tsx
```

---

## Hooks

```txt
useExample
```

Exemplos:

```txt
useAuth
useProposal
useVoting
```

---

## Pastas

```txt
kebab-case
```

Exemplos:

```txt
proposal-card
vote-modal
dashboard-layout
```

---

# 21. Estratégia de Performance

Práticas recomendadas:

- Lazy loading
- Code splitting
- Memoização
- Virtualização futura
- Compressão de assets
- Cache PWA
- Otimização de imagens

---

# 22. Estratégia de Testes

Tipos de testes recomendados:

| Tipo | Objetivo |
|---|---|
| Unitário | Componentes e hooks |
| Integração | Features |
| E2E | Fluxos completos |

Ferramentas futuras:

- Vitest
- React Testing Library
- Playwright

---

# 23. Ordem Recomendada de Desenvolvimento

## Fase 1 — Foundation

- Estrutura React
- Configuração Vite
- PWA
- Router
- Store
- Axios

---

## Fase 2 — Core

- Auth
- Layouts
- Dashboard
- Navegação
- Guards de rota

---

## Fase 3 — Produto

- Proposals
- Voting
- Debate
- Setup
- Notifications

---

## Fase 4 — Avançado

- Auditoria
- Estatísticas
- Admin
- Internacionalização completa
- Offline avançado

---

# 24. O que NÃO fazer agora

Evitar nesta fase:

- SSR complexo
- Microfrontends
- Excessos de abstração
- State management excessivo
- Bibliotecas desnecessárias
- Over-engineering
- Design system complexo prematuramente

---

# 25. Conclusão

A arquitetura frontend do Democracy Without Candidates deve priorizar:

- Organização
- Escalabilidade
- Mobile-first
- Reutilização
- Performance
- Acessibilidade
- Simplicidade estrutural
- Crescimento sustentável

O foco inicial deve ser construir uma base sólida antes de otimizações avançadas.

---

# Democracy Without Candidates
## Frontend Architecture Guide
