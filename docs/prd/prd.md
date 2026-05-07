# Democracy Without Candidates
## Product Requirements Document (PRD)

> **Versão:** 1.1.0 | **Data:** Maio de 2026 | **Status:** Em Desenvolvimento Ativo

---

## Sumário

1. [Visão Geral do Produto](#1-visão-geral-do-produto)
2. [Arquitetura Multi-País](#2-arquitetura-multi-país)
3. [Módulo: Autenticação Biométrica](#3-módulo-autenticação-biométrica)
4. [Módulo: RBAC — Controle de Acesso por Papel](#4-módulo-rbac--controle-de-acesso-por-papel)
5. [Módulo: Propostas e Leis](#5-módulo-propostas-e-leis)
6. [Módulo: Votação](#6-módulo-votação)
7. [Módulo: Dashboard](#7-módulo-dashboard)
8. [Módulo: Auditoria e Transparência](#8-módulo-auditoria-e-transparência)
9. [Módulo: API](#9-módulo-api)
10. [Requisitos Não Funcionais](#10-requisitos-não-funcionais)
11. [Mapa de Telas (31 Telas)](#11-mapa-de-telas-31-telas)
12. [Critérios de Aceite](#12-critérios-de-aceite)

---

## 1. Visão Geral do Produto

O **Democracy Without Candidates** é uma plataforma web multi-país de democracia direta digital que permite a qualquer cidadão autenticado propor, debater e votar leis e decisões coletivas — sem intermediários, representantes ou candidatos. O sistema elimina a figura do candidato e do partido, devolvendo ao cidadão o poder de legislar diretamente.

A plataforma opera com autenticação biométrica facial obrigatória combinada com segundo fator (2FA via TOTP/Google Authenticator), garantindo **1 pessoa = 1 voto**. Privacidade configurável por cidadão e regras de funcionamento definidas pelo próprio povo de cada país através de um plebiscito de configuração inicial (setup).

### 1.1 Pilares do Sistema

| Pilar | Descrição |
|-------|-----------|
| **1 pessoa = 1 voto** | Autenticação facial + 2FA impede duplicidade |
| **Soberania do setup** | Cada jurisdição define suas próprias regras via votação direta |
| **Privacidade por padrão** | Voto secreto como padrão, com opt-in público |
| **Transparência total** | Resultados, estatísticas e auditorias sempre públicos |
| **Código auditável** | Plataforma open-source ou com auditoria independente obrigatória |

### 1.2 Stack Técnica

| Camada | Tecnologia | Detalhes |
|--------|-----------|---------|
| **Backend** | NestJS + TypeORM | Arquitetura MVC, Node 24/26 |
| **Frontend** | React (PWA) | Responsivo, mobile-first, instalável |
| **Banco de Dados** | PostgreSQL | Isolamento por schema por jurisdição |
| **Autenticação** | Face Recognition + 2FA | [auth-face](https://github.com/MuriloCarvalhoo/auth-face) + TOTP (Google Authenticator) |
| **Permissões** | RBAC | Role-Based Access Control |
| **Repositório** | Monorepo (pnpm) | Node 24 ou 26, workspace pnpm |
| **Infra Local** | Docker + Docker Compose | Produção: a definir |
| **Criptografia** | TLS 1.3 + AES-256 + ZKP | Zero-knowledge para votos privados |

### 1.3 Tipos de Proposta

- Lei Normal
- Lei Urgente
- Proposta de Alteração do Setup (Meta-lei)
- Plebiscito de Configuração Inicial

---

## 2. Arquitetura Multi-País

### 2.1 Hierarquia de Dados

O sistema atende múltiplas jurisdições independentes com total isolamento de dados entre elas:

```
Admin Master (plataforma global)
└── País / Jurisdição (ex: Brasil, Portugal, Argentina)
    └── Nível Nacional > Estadual > Municipal
```

### 2.2 Regras de Isolamento

| Regra | Descrição |
|-------|-----------|
| **Isolamento por País** | Dados, votos e propostas de um país nunca são misturados com outro |
| **Isolamento por Nível** | Propostas municipais não interferem em estaduais ou nacionais |
| **Cidadão por Jurisdição** | Um cidadão é registrado em uma jurisdição por vez |
| **Setup por País** | Cada país tem seu próprio conjunto de regras definido por plebiscito |
| **Imutabilidade de Votos** | Votos emitidos não podem ser alterados, apenas auditados |

### 2.3 Parâmetros Padrão Sugeridos

| Parâmetro | Valor Padrão |
|-----------|-------------|
| Assinaturas — proposta nacional | 0,5% do eleitorado |
| Assinaturas — proposta estadual | 1% do eleitorado estadual |
| Assinaturas — proposta municipal | 2% do eleitorado municipal |
| Quórum mínimo — leis normais | 25% dos eleitores aptos |
| Quórum mínimo — leis urgentes | 20% dos eleitores aptos |
| Quórum — configuração inicial | 40% dos eleitores aptos |
| Maioria — configuração inicial | 60% + 1 dos votos válidos |
| Quórum — alteração de setup | 35% dos eleitores aptos |
| Maioria — alteração de setup | 60% + 1 dos votos válidos |
| Prazo coleta assinaturas — normal | 90 dias |
| Prazo debate — normal | 15 dias |
| Prazo votação — normal | 30 dias |
| Prazo coleta assinaturas — urgente | 15 dias |
| Prazo votação — urgente | 7 dias |
| Limite propostas normais/cidadão/ano | 5 |
| Limite propostas urgentes/cidadão/ano | 1 |

---

## 3. Módulo: Autenticação Biométrica

A autenticação utiliza reconhecimento facial combinado com segundo fator (2FA). A implementação base utiliza o repositório [auth-face](https://github.com/MuriloCarvalhoo/auth-face) como ponto de partida, com evolução prevista.

### 3.1 Fluxo de Autenticação

1. Usuário informa e-mail
2. Captura facial via câmera do dispositivo
3. Validação do hash biométrico facial
4. Inserção do código TOTP (Google Authenticator ou similar)
5. Emissão de JWT com escopo e permissões RBAC

### 3.2 Reautenticação para Ações Críticas

As seguintes ações exigem revalidação biométrica completa (face + 2FA):

- Emissão de voto
- Assinatura de proposta
- Criação de proposta
- Alteração de dados de perfil sensíveis

### 3.3 Segurança Biométrica

| Aspecto | Implementação |
|---------|--------------|
| **Armazenamento** | Hash biométrico — nunca imagem bruta |
| **Transmissão** | TLS 1.3 — criptografado em trânsito |
| **Repouso** | AES-256 — criptografado no banco |
| **Unicidade** | 1 hash biométrico por CPF/documento por jurisdição |

---

## 4. Módulo: RBAC — Controle de Acesso por Papel

| Papel (Role) | Descrição | Permissões Principais |
|-------------|-----------|----------------------|
| **Visitante** | Não autenticado | Leitura pública (propostas, resultados, estatísticas) |
| **Cidadão Pendente** | Cadastrado, biometria pendente | Completar cadastro |
| **Cidadão** | Autenticado e validado | Propor, assinar, votar, comentar, perfil |
| **Auditor Independente** | Credenciado para auditoria | Logs avançados, provas criptográficas (somente leitura) |
| **Admin País** | Administrador de uma jurisdição | Gerenciar configurações do país |
| **Admin Master** | Administrador da plataforma | Acesso global, cadastro de países e auditores |

---

## 5. Módulo: Propostas e Leis

### 5.1 Status de uma Proposta

| Status | Descrição |
|--------|-----------|
| **Rascunho** | Criada pelo autor, não publicada |
| **Coleta de Assinaturas** | Publicada, coletando assinaturas |
| **Em Debate** | Assinaturas atingidas, período de debate ativo |
| **Em Votação** | Debate encerrado, votação aberta |
| **Aprovada** | Quórum e maioria atingidos |
| **Rejeitada** | Maioria não atingida |
| **Quórum Não Atingido** | Votação encerrada sem quórum suficiente |
| **Expirada** | Prazo de assinaturas esgotado sem atingir meta |
| **Arquivada** | Removida de circulação ativa |

### 5.2 Limites por Cidadão por Ano

- Propostas normais: **5 por ano**
- Propostas urgentes: **1 por ano**
- Limite bloqueante — sistema impede criação após atingir o limite

---

## 6. Módulo: Votação

### 6.1 Opções de Voto

- ✅ **SIM**
- ❌ **NÃO**
- ⚪ **ABSTENÇÃO**

### 6.2 Privacidade do Voto

- **Padrão:** Voto privado (zero-knowledge proof)
- **Opt-in:** Voto público (vinculado ao cidadão com consentimento)
- Configurável por cidadão a cada votação

### 6.3 Cálculo de Resultado

| Verificação | Critério |
|------------|---------|
| **Quórum** | % de eleitores aptos que votaram >= mínimo configurado |
| **Maioria** | % de votos válidos (SIM vs NÃO, excluindo abstenções) |
| **Resultado** | Aprovada (quórum + maioria) \| Rejeitada \| Quórum não atingido |

---

## 7. Módulo: Dashboard

| Perfil | Conteúdo Exibido |
|--------|-----------------|
| **Visitante** | Votações ativas, propostas públicas, estatísticas da jurisdição |
| **Cidadão** | Tudo do visitante + Minhas propostas, Minhas assinaturas, Notificações |
| **Admin Master** | Estatísticas globais, saúde do sistema, gestão de países |

---

## 8. Módulo: Auditoria e Transparência

| Evento | Dados Registrados |
|--------|------------------|
| Criação de proposta | ID cidadão, ID proposta, timestamp, hash do texto |
| Assinatura de proposta | ID cidadão (ou hash anônimo), ID proposta, timestamp |
| Emissão de voto | ZKP (privado) ou ID cidadão + voto (público) |
| Alteração de setup | Parâmetro, valor anterior, valor novo, ID meta-lei |
| Criação de conta | Hash biométrico, timestamp, jurisdição |
| Acesso de auditor | ID auditor, dados acessados, timestamp |

---

## 9. Módulo: API

### 9.1 Endpoints Públicos (sem autenticação)

| Endpoint | Descrição |
|----------|-----------|
| `GET /api/v1/paises` | Lista países ativos |
| `GET /api/v1/paises/:id/propostas` | Lista propostas de uma jurisdição |
| `GET /api/v1/propostas/:id` | Detalhe de uma proposta |
| `GET /api/v1/propostas/:id/resultado` | Resultado de uma votação encerrada |
| `GET /api/v1/paises/:id/estatisticas` | Estatísticas agregadas de uma jurisdição |

### 9.2 Endpoints Autenticados (JWT + Biometria)

| Endpoint | Descrição |
|----------|-----------|
| `POST /api/v1/auth/registro` | Registro de novo cidadão |
| `POST /api/v1/auth/login` | Autenticação biométrica + 2FA |
| `POST /api/v1/propostas` | Criar proposta |
| `POST /api/v1/propostas/:id/assinar` | Assinar proposta |
| `POST /api/v1/propostas/:id/votar` | Emitir voto |
| `POST /api/v1/propostas/:id/comentarios` | Comentar no debate |
| `GET /api/v1/cidadao/perfil` | Dados do cidadão autenticado |
| `PUT /api/v1/cidadao/perfil` | Atualizar perfil |

---

## 10. Requisitos Não Funcionais

| ID | Requisito | Detalhe |
|----|-----------|---------|
| RNF01 | Disponibilidade | 99,9% de uptime — manutenção fora de votações ativas |
| RNF02 | Performance | Listagens < 2s, votação processa < 3s |
| RNF03 | Segurança biométrica | Hash facial — nunca armazenamento de imagem bruta |
| RNF04 | Criptografia | TLS 1.3 em trânsito, AES-256 em repouso, ZKP para votos privados |
| RNF05 | Escalabilidade | Suporte a picos de votação com milhões de votos simultâneos |
| RNF06 | Auditabilidade | Blockchain ou equivalente para imutabilidade de registros críticos |
| RNF07 | Open source | Código auditável publicamente ou por auditores credenciados |
| RNF08 | Acessibilidade | WCAG 2.1 AA — leitores de tela, alto contraste, teclado |
| RNF09 | Internacionalização | i18n — múltiplos idiomas, datas e fusos por jurisdição |
| RNF10 | LGPD / GDPR | Conformidade com legislações de proteção de dados |
| RNF11 | Backup | Backup diário com retenção mínima de 10 anos |
| RNF12 | Monorepo | NestJS + React + libs compartilhadas em monorepo pnpm |
| RNF13 | Stack técnica | Backend: NestJS + TypeORM \| Frontend: React PWA \| DB: PostgreSQL |
| RNF14 | PWA | Frontend instalável como app no celular e desktop |
| RNF15 | Docker | Docker + Docker Compose para subir infra local |

---

## 11. Mapa de Telas (31 Telas)

| Módulo | Tela | Rota | Acesso |
|--------|------|------|--------|
| Autenticação | Login / Auth Biométrica | `/login` | Público |
| Autenticação | Cadastro de Cidadão | `/cadastro` | Público |
| Autenticação | Validação Biométrica | `/cadastro/biometria` | Cidadão Pendente |
| Autenticação | Troca de Senha / PIN | `/alterar-senha` | Cidadão Autenticado |
| Admin | Listagem de Países | `/admin/paises` | Admin Master |
| Admin | Cadastro / Edição de País | `/admin/paises/novo` \| `/:id` | Admin Master |
| Admin | Gestão de Auditores | `/admin/auditores` | Admin Master |
| Perfil | Meu Perfil | `/perfil` | Cidadão Autenticado |
| Perfil | Configurações de Privacidade | `/perfil/privacidade` | Cidadão Autenticado |
| Perfil | Preferências de Notificação | `/perfil/notificacoes` | Cidadão Autenticado |
| Propostas | Listagem de Propostas | `/propostas` | Todos |
| Propostas | Detalhe da Proposta | `/propostas/:id` | Todos |
| Propostas | Criar Proposta | `/propostas/nova` | Cidadão Autenticado |
| Propostas | Editar Proposta (rascunho) | `/propostas/:id/editar` | Autor |
| Propostas | Exportar Proposta | `/propostas/:id/exportar` | Todos |
| Assinaturas | Assinar Proposta | `/propostas/:id/assinar` | Cidadão Autenticado |
| Assinaturas | Lista de Assinantes | `/propostas/:id/assinantes` | Todos |
| Debate | Debate da Proposta | `/propostas/:id/debate` | Todos (comentar: auth) |
| Debate | Propor Emenda | `/propostas/:id/debate/emenda` | Cidadão Autenticado |
| Votação | Votar na Proposta | `/propostas/:id/votar` | Cidadão Autenticado |
| Votação | Resultado da Votação | `/propostas/:id/resultado` | Todos |
| Meta-lei | Listagem de Meta-leis | `/metaleis` | Todos |
| Meta-lei | Criar Meta-lei | `/metaleis/nova` | Cidadão Autenticado |
| Meta-lei | Detalhe / Votação Meta-lei | `/metaleis/:id` | Todos |
| Setup | Plebiscito Inicial | `/setup/:paisId` | Cidadão Autenticado |
| Setup | Histórico do Setup | `/setup/:paisId/historico` | Todos |
| Dashboard | Dashboard Principal | `/dashboard` | Todos |
| Notificações | Central de Notificações | `/notificacoes` | Cidadão Autenticado |
| Auditoria | Log de Auditoria Pública | `/auditoria` | Todos |
| Auditoria | Auditoria Avançada | `/auditoria/avancada` | Auditor / Admin |
| Estatísticas | Estatísticas por País | `/estatisticas/:paisId` | Todos |

---

## 12. Critérios de Aceite

| ID | Critério |
|----|---------|
| R01 | Sistema acessível via WEB (PWA) com disponibilidade em tempo real |
| R02 | Autenticação biométrica facial + 2FA funcional: 1 pessoa = 1 conta |
| R03 | RBAC implementado com todos os 6 papéis definidos |
| R04 | Perfil público e privado configurável pelo cidadão |
| R05 | Voto privado (ZKP) e público implementados com garantias criptográficas |
| R06 | Plebiscito de configuração inicial — nenhuma lei criada antes do setup |
| R07 | Parâmetros do setup só alteráveis via meta-lei com quórum/maioria especiais |
| R08 | Fluxo completo de proposta com todos os 9 status |
| R09 | Coleta de assinaturas com contador em tempo real e validação biométrica |
| R10 | Período de debate com comentários, emendas e moderação |
| R11 | Período de votação com reautenticação biométrica obrigatória |
| R12 | Cálculo automático de quórum e maioria ao encerrar votação |
| R13 | Isolamento total de dados entre jurisdições |
| R14 | Limite de propostas por cidadão por ano funcional e bloqueante |
| R15 | Dashboard funcional para cidadão, visitante e Admin Master |
| R16 | Sistema de notificações in-app e e-mail funcional |
| R17 | Preferências de notificação configuráveis por cidadão |
| R18 | API pública com endpoints de listagem e resultados |
| R19 | API autenticada com endpoints de criação, assinatura e votação |
| R20 | Módulo de auditoria com registro imutável de todos os eventos críticos |
| R21 | Auditores independentes verificam integridade sem acessar votos privados |
| R22 | Backup diário implementado e auditável |
| R23 | Conformidade com WCAG 2.1 AA |
| R24 | Suporte a múltiplos idiomas (i18n) |
| R25 | Docker Compose funcional para subir infra local completa |
| R26 | Frontend instalável como PWA no celular e desktop |

---

*Democracy Without Candidates — PRD v1.1.0 — Maio 2026*
