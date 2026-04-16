# PRD — Product Requirements Document
## Democracy Without Candidates — Plataforma de Democracia Direta Digital Multi-País

## Sumário

1. [Visão Geral do Produto](#1-visão-geral-do-produto)
2. [Arquitetura Multi-País](#2-arquitetura-multi-país)
3. [Módulo: Gestão de Países e Configuração Inicial](#3-módulo-gestão-de-países-e-configuração-inicial)
4. [Módulo: Autenticação Biométrica e Perfil do Cidadão](#4-módulo-autenticação-biométrica-e-perfil-do-cidadão)
5. [Módulo: Privacidade e Configurações de Voto](#5-módulo-privacidade-e-configurações-de-voto)
6. [Módulo: Propostas e Leis](#6-módulo-propostas-e-leis)
7. [Módulo: Coleta de Assinaturas](#7-módulo-coleta-de-assinaturas)
8. [Módulo: Debate e Discussão](#8-módulo-debate-e-discussão)
9. [Módulo: Votação](#9-módulo-votação)
10. [Módulo: Meta-Regras e Configuração do Setup](#10-módulo-meta-regras-e-configuração-do-setup)
11. [Personas e Perfis de Acesso](#11-personas-e-perfis-de-acesso)
12. [Fluxo Operacional de Ponta a Ponta](#12-fluxo-operacional-de-ponta-a-ponta)
13. [Módulo: Dashboard](#13-módulo-dashboard)
14. [Módulo: Notificações](#14-módulo-notificações)
15. [Módulo: Auditoria e Transparência](#15-módulo-auditoria-e-transparência)
16. [Módulo: Integração via API](#16-módulo-integração-via-api)
17. [Requisitos Não Funcionais](#17-requisitos-não-funcionais)
18. [Critérios de Aceite](#18-critérios-de-aceite)
19. [Mapa de Telas](#19-mapa-de-telas)

---

## 1. Visão Geral do Produto

O **Democracy Without Candidates** é uma plataforma web **multi-país** de democracia direta digital que permite a qualquer cidadão autenticado propor, debater e votar leis e decisões coletivas — sem intermediários, representantes ou candidatos. O sistema elimina a figura do candidato e do partido, devolvendo ao cidadão o poder de legislar diretamente.

A plataforma opera com autenticação biométrica obrigatória (garantindo 1 pessoa = 1 voto), privacidade configurável por cidadão, e regras de funcionamento definidas pelo próprio povo de cada país através de um **plebiscito de configuração inicial** (setup). Toda alteração nas regras do jogo exige votação especial com quorum elevado e maioria qualificada.

**Objetivo Principal:** Prover uma infraestrutura digital neutra, auditável e segura para o exercício da democracia direta em qualquer país, estado ou município, com total isolamento de dados entre jurisdições, rastreabilidade de ponta a ponta e garantia criptográfica da integridade dos votos.

**Pilares do Sistema:**
- **1 pessoa = 1 voto:** Autenticação biométrica irreversível impede duplicidade
- **Soberania do setup:** Cada jurisdição define suas próprias regras via votação direta
- **Privacidade por padrão:** Voto secreto como padrão, com opt-in público
- **Transparência total:** Resultados, estatísticas e auditorias sempre públicos
- **Código auditável:** Plataforma open-source ou com auditoria independente obrigatória

**Tipos de Proposta:**
- Lei Normal
- Lei Urgente
- Proposta de Alteração do Setup (Meta-lei)
- Plebiscito de Configuração Inicial

---

## 2. Arquitetura Multi-País

### 2.1. Conceito Geral

O sistema atende múltiplas jurisdições independentes (países, estados, municípios). A hierarquia de dados é:

```
Admin Master (plataforma)
 └── País / Jurisdição (Ex: Brasil, Portugal, Argentina)
      ├── Configuração do País (Setup — definido por votação)
      │    ├── Separação de competências (nacional/estadual/municipal)
      │    ├── Percentuais de assinaturas por nível
      │    ├── Quorum mínimo (normal e urgente)
      │    ├── Limites de propostas por ano por cidadão
      │    └── Regras de urgência, debate e votação
      ├── Nível Nacional
      │    ├── Cidadãos registrados
      │    ├── Propostas Nacionais
      │    └── Votações Nacionais
      ├── Nível Estadual
      │    └── (mesma estrutura, escopo estadual)
      └── Nível Municipal
           └── (mesma estrutura, escopo municipal)
```

### 2.2. Regras de Isolamento

| Regra | Descrição |
|---|---|
| Isolamento por País | Dados, votos e propostas de um país nunca são visíveis ou misturados com outro |
| Isolamento por Nível | Propostas municipais não interferem em estaduais ou nacionais |
| Cidadão por Jurisdição | Um cidadão é registrado em uma jurisdição por vez (país); subnítveis derivam do registro nacional |
| Setup por País | Cada país tem seu próprio conjunto de regras definido pelo plebiscito de configuração |
| Imutabilidade de Votos | Votos emitidos não podem ser alterados, apenas auditados |

### 2.3. Parâmetros Padrão Sugeridos (Pré-configurados, alteráveis apenas por votação)

| Parâmetro | Valor Padrão Sugerido |
|---|---|
| Assinaturas para proposta nacional | 0,5% do eleitorado |
| Assinaturas para proposta estadual | 1% do eleitorado estadual |
| Assinaturas para proposta municipal | 2% do eleitorado municipal |
| Quorum mínimo — leis normais | 25% dos eleitores aptos |
| Quorum mínimo — leis urgentes | 20% dos eleitores aptos |
| Quorum — configuração inicial | 40% dos eleitores aptos |
| Maioria — configuração inicial | 60% + 1 dos votos válidos |
| Quorum — alteração de setup (meta-lei) | 35% dos eleitores aptos |
| Maioria — alteração de setup (meta-lei) | 60% + 1 dos votos válidos |
| Prazo coleta de assinaturas — normal | 90 dias |
| Prazo debate — normal | 15 dias |
| Prazo votação — normal | 30 dias |
| Prazo coleta de assinaturas — urgente | 15 dias |
| Prazo votação — urgente | 7 dias |
| Limite de propostas normais por cidadão/ano | 5 |
| Limite de propostas urgentes por cidadão/ano | 1 |

---

## 3. Módulo: Gestão de Países e Configuração Inicial

### 3.1. Tela: Listagem de Países/Jurisdições

Acessível exclusivamente pelo **Admin Master**. Lista todas as jurisdições cadastradas na plataforma.

**Funcionalidades:**
- Filtros por: nome, status (aguardando setup / ativo / inativo)
- Ações: Visualizar, Editar, Ativar/Inativar
- Indicação de status do setup (plebiscito realizado ou pendente)
- Indicação de quantidade de cidadãos registrados
- Indicação de quantidade de propostas ativas

---

### 3.2. Tela: Cadastro/Edição de País

Formulário para criar ou editar uma jurisdição.

**Campos:**

| Campo | Tipo | Obrigatório | Editável | Descrição |
|---|---|---|---|---|
| Nome do País | String | Sim | Sim | Ex: "Brasil", "Portugal" |
| Código ISO | String | Sim | Não (após criação) | ISO 3166-1 alpha-2. Único no sistema. |
| Idioma principal | Enum | Sim | Sim | Idioma da interface para cidadãos deste país |
| Fuso horário | String | Sim | Sim | Fuso padrão para prazos e registros |
| API Gov (biometria) | String | Não | Sim | Endpoint de integração com sistema de identidade governamental |
| Status | Enum | Sim | Sim | Aguardando Setup / Ativo / Inativo |

---

### 3.3. Plebiscito de Configuração Inicial

Ao criar um país, o sistema inicia automaticamente o **plebiscito de setup**. Nenhuma proposta de lei pode ser criada antes da conclusão do setup.

**Fluxo:**
1. Admin Master cria o país
2. Sistema abre período de registro de cidadãos (prazo configurável pelo Admin Master na criação)
3. Após período de registro, sistema abre votação de configuração inicial com parâmetros padrão sugeridos
4. Cidadãos votam os parâmetros (podem aceitar os padrões ou propor alterações durante o debate)
5. Votação encerra com quorum 40% + maioria 60%+1
6. Parâmetros aprovados ficam gravados imutavelmente como o setup do país
7. Sistema ativa o país para operação normal

---

## 4. Módulo: Autenticação Biométrica e Perfil do Cidadão

### 4.1. Autenticação

O sistema exige autenticação biométrica obrigatória para garantir **1 pessoa = 1 conta irreversível**.

**Métodos aceitos:**
- Reconhecimento facial com detecção de vivacidade (liveness detection)
- Impressão digital (via dispositivo compatível)
- Integração com APIs governamentais de identidade (ex: gov.br, DNI eletrônico, eID europeu)
- Escaneamento de documento oficial (fallback com validação manual)

**Regras:**
- Hash biométrico armazenado de forma segura — nunca a imagem bruta
- Uma biometria = uma conta. Impossível criar segunda conta com mesma biometria
- Conta criada é irreversível (pode ser inativada por ordem judicial, nunca excluída)
- Reautenticação biométrica exigida para: emissão de voto, criação de proposta, coleta de assinatura

### 4.2. Tela: Cadastro do Cidadão

**Campos:**

| Campo | Tipo | Obrigatório | Editável | Descrição |
|---|---|---|---|---|
| Nome completo | String | Sim | Não (após verificação) | Vinculado ao documento oficial |
| CPF / ID Nacional | String | Sim | Não | Documento de identidade nacional. Único. |
| Data de nascimento | Date | Sim | Não | Para validação de elegibilidade (ex: 16+ anos) |
| País / Jurisdição | Enum | Sim | Não | Jurisdição de registro |
| Biometria | Biometric | Sim | Não | Hash seguro — facial ou digital |
| E-mail | String | Não | Sim | Para notificações (opcional) |
| Tipo de Perfil | Enum | Sim | Sim | Público ou Privado |
| Foto de perfil | Image | Não | Sim | Apenas se perfil público |
| Bio | Text | Não | Sim | Apenas se perfil público |

### 4.3. Tipos de Perfil

| Tipo | Visibilidade para outros cidadãos |
|---|---|
| Público | Nome real (ou pseudônimo verificado), foto, bio, histórico de propostas, votos públicos |
| Privado | Somente ID numérico interno ou "Cidadão Anônimo" — CPF/biometria nunca expostos |

> **Regra:** O cidadão pode alterar o tipo de perfil a qualquer momento. Votos já emitidos mantêm a configuração de privacidade original do momento da emissão.

---

## 5. Módulo: Privacidade e Configurações de Voto

### 5.1. Privacidade do Voto

Cada cidadão, **no momento de cada votação**, escolhe individualmente:

| Tipo de Voto | Descrição |
|---|---|
| Voto Privado (padrão) | Apenas o voto (Sim/Não/Abstenção) entra na contagem. Nenhum sistema ou pessoa associa o voto ao eleitor após emissão. Implementado via criptografia zero-knowledge ou equivalente auditável. |
| Voto Público (opt-in) | Nome/perfil + voto registrado publicamente. Aumenta accountability. Útil para cargos ou debates de transparência. |

**Padrão:** Privado. O cidadão precisa optar ativamente pelo voto público.

**Estatísticas sempre públicas:** Percentual por região, por faixa etária (se autorizado pelo cidadão no perfil), total de votos — nunca vinculados individualmente no modo privado.

### 5.2. Garantias Técnicas de Privacidade

| Garantia | Descrição |
|---|---|
| Zero-knowledge proof | Voto privado pode ser verificado como válido sem revelar o eleitor |
| Imutabilidade | Voto emitido não pode ser alterado nem pelo cidadão nem pelo sistema |
| Auditabilidade | Qualquer auditor independente pode verificar a contagem sem acessar votos individuais privados |
| Segregação de dados | Hash biométrico e voto nunca armazenados no mesmo registro |

---

## 6. Módulo: Propostas e Leis

### 6.1. Tipos de Proposta

| Tipo | Prazo Coleta | Prazo Debate | Prazo Votação | Quorum | Maioria |
|---|---|---|---|---|---|
| Lei Normal | 90 dias | 15 dias | 30 dias | 25% | Simples |
| Lei Urgente | 15 dias | — | 7 dias | 20% | Simples |
| Meta-lei (alteração de setup) | Configurável | 30 dias | 30 dias | 35% | 60%+1 |

### 6.2. Tela: Listagem de Propostas

**Funcionalidades:**
- Filtros por: tipo, status, nível (nacional/estadual/municipal), tema, autor (se público), período
- Ordenação por: mais recentes, mais assinaturas, votação mais próxima
- Indicação visual de status (Ex: badge colorido por fase)
- Busca por texto livre no título e resumo

**Status possíveis:**

| Status | Descrição |
|---|---|
| RASCUNHO | Proposta criada mas não publicada pelo autor |
| COLETA DE ASSINATURAS | Publicada, em período de coleta |
| ASSINATURAS INSUFICIENTES | Prazo encerrado sem atingir o mínimo — arquivada |
| EM DEBATE | Assinaturas suficientes, período de debate aberto |
| EM VOTAÇÃO | Período de votação aberto |
| APROVADA | Votação encerrada, quorum atingido, maioria favorável |
| REJEITADA | Votação encerrada, quorum atingido, maioria contrária |
| QUORUM NÃO ATINGIDO | Votação encerrada sem quorum mínimo — nula |
| CANCELADA | Retirada pelo autor (apenas na fase de rascunho ou coleta) |

### 6.3. Tela: Criação de Proposta

**Campos:**

| Campo | Tipo | Obrigatório | Descrição |
|---|---|---|---|
| Título | String (max 200 chars) | Sim | Título claro e objetivo |
| Tipo | Enum | Sim | Normal / Urgente / Meta-lei |
| Nível | Enum | Sim | Nacional / Estadual / Municipal |
| Estado/Município | Enum | Condicional | Obrigatório se nível não for nacional |
| Tema / Categoria | Enum | Sim | Ex: Saúde, Educação, Meio Ambiente, Segurança, Economia, etc. |
| Resumo | Text (max 500 chars) | Sim | Explicação simples em linguagem acessível |
| Texto completo | Rich Text | Sim | Texto integral da proposta de lei |
| Justificativa | Text | Sim | Por que essa lei é necessária |
| Urgência (se urgente) | Text | Condicional | Justificativa obrigatória para propostas urgentes |
| Documentos anexos | File | Não | PDFs, estudos, dados de suporte |

**Regras:**
- Cidadão autenticado pode criar proposta (reautenticação biométrica obrigatória)
- Limite: 5 propostas normais + 1 urgente por ano por cidadão (conforme setup)
- Proposta em rascunho pode ser editada livremente
- Após publicação (início da coleta), apenas correções ortográficas são permitidas (com registro de auditoria)
- Cancelamento permitido apenas nas fases RASCUNHO e COLETA DE ASSINATURAS

---

## 7. Módulo: Coleta de Assinaturas

### 7.1. Funcionamento

Após publicação da proposta, inicia-se o período de coleta de assinaturas. O sistema exibe em tempo real o contador de assinaturas e a barra de progresso até a meta.

**Regras:**
- Cidadão assina com reautenticação biométrica
- 1 assinatura por cidadão por proposta
- Assinatura pode ser retirada durante o período de coleta (mas não após)
- O nome do assinante é público ou anônimo conforme configuração do perfil do cidadão
- Ao atingir o percentual mínimo (conforme setup), a proposta avança automaticamente para a fase de debate
- Se o prazo encerrar sem atingir o mínimo: status muda para ASSINATURAS INSUFICIENTES e a proposta é arquivada

### 7.2. Tela: Detalhe da Proposta (fase coleta)

- Exibe texto completo, resumo, justificativa
- Contador em tempo real de assinaturas (ex: "47.832 de 106.000 necessárias — 45%")
- Barra de progresso visual
- Botão de assinatura (com confirmação biométrica)
- Lista de assinantes públicos (paginada)
- Prazo restante em destaque

---

## 8. Módulo: Debate e Discussão

### 8.1. Funcionamento

Após atingir o mínimo de assinaturas, a proposta entra no período de debate. Cidadãos autenticados podem comentar, propor emendas e discutir o mérito.

**Funcionalidades:**
- Comentários em árvore (resposta a comentário)
- Upvote/downvote em comentários (para destacar os mais relevantes)
- Proposta de emenda ao texto (sugestão de alteração com justificativa)
- Relatório de abuso (moderação por denúncia)
- O autor da proposta pode responder formalmente às emendas
- Emendas aprovadas por votação interna do debate podem ser incorporadas ao texto final (conforme setup)

**Regras:**
- Apenas cidadãos autenticados podem comentar ou propor emendas
- Comentários e emendas têm autoria pública (nome do cidadão ou ID anônimo, conforme perfil)
- Sem censura prévia — moderação apenas por denúncia e análise posterior
- Período de debate tem prazo fixo (conforme setup do país)

---

## 9. Módulo: Votação

### 9.1. Fluxo de Votação

Após o período de debate, a proposta entra automaticamente em votação.

**Opções de voto:**
- **SIM** — favorável à proposta
- **NÃO** — contrário à proposta
- **ABSTENÇÃO** — registrado na contagem, não conta para maioria

**Regras:**
- Reautenticação biométrica obrigatória para votar
- 1 voto por cidadão por proposta — irreversível após confirmação
- Cidadão escolhe: voto público ou privado (no momento do voto)
- Resultados parciais: o sistema pode ser configurado (via setup) para exibir ou ocultar parciais durante o período de votação
- Ao encerrar o prazo: sistema calcula quorum, maioria e determina o resultado automaticamente

### 9.2. Cálculo de Resultado

| Condição | Resultado |
|---|---|
| Quorum não atingido | QUORUM NÃO ATINGIDO — proposta nula |
| Quorum atingido + maioria SIM | APROVADA |
| Quorum atingido + maioria NÃO | REJEITADA |
| Empate | REJEITADA (em caso de empate, a proposta não passa) |

### 9.3. Tela: Votação

- Exibe resumo da proposta, texto completo e resultado do debate
- Botão de voto: SIM / NÃO / ABSTENÇÃO
- Seleção de privacidade do voto (público ou privado) — padrão: privado
- Confirmação com reautenticação biométrica
- Após voto: exibe estatísticas gerais (se setup permitir parciais)

---

## 10. Módulo: Meta-Regras e Configuração do Setup

### 10.1. Proposta de Alteração do Setup

Qualquer alteração nos parâmetros do setup (quorum, prazos, percentuais, etc.) só pode ser realizada via **Proposta de Meta-lei**. Essas propostas seguem regras especiais mais rígidas para evitar mudanças impulsivas nas regras do jogo.

**Fluxo:**
1. Cidadão cria Proposta de Meta-lei especificando o parâmetro a alterar e o novo valor proposto
2. Coleta de assinaturas com percentual mais alto (definido no setup)
3. Período de debate obrigatório (mínimo 30 dias, configurável)
4. Votação especial: quorum 35% + maioria 60%+1
5. Se aprovada: parâmetro alterado imediatamente no setup do país, com registro imutável de auditoria

**Regras:**
- Propostas de meta-lei não se misturam com propostas de leis comuns
- Apenas um parâmetro por proposta (para clareza)
- Histórico completo de todas as alterações de setup é público e auditável
- Parâmetros aprovados no setup inicial não podem ser retroativos

---

## 11. Personas e Perfis de Acesso

| Perfil | Descrição | Permissões |
|---|---|---|
| **Admin Master** | Equipe da plataforma Democracy Without Candidates | Gestão de países, configurações técnicas globais, auditoria da plataforma |
| **Cidadão Autenticado** | Pessoa registrada e com biometria validada em uma jurisdição | Criar propostas, assinar, debater, votar |
| **Cidadão Pendente** | Registro iniciado mas biometria ainda não validada | Apenas visualização — sem interação |
| **Visitante** | Não registrado | Visualização de propostas públicas, estatísticas e resultados — sem interação |
| **Auditor Independente** | Entidade externa credenciada pelo Admin Master | Acesso a dados agregados e provas criptográficas para auditoria — sem dados individuais privados |

---

## 12. Fluxo Operacional de Ponta a Ponta

```
[Cidadão]
    │
    ├─► Registro + Autenticação Biométrica
    │         └── 1 pessoa = 1 conta irreversível
    │
    ├─► Criação de Proposta (reautenticação biométrica)
    │         └── Rascunho → Publicação → COLETA DE ASSINATURAS
    │
    ├─► Coleta de Assinaturas
    │         ├── Atingiu mínimo → EM DEBATE
    │         └── Prazo expirou sem mínimo → ASSINATURAS INSUFICIENTES (arquivada)
    │
    ├─► Debate
    │         └── Prazo encerrado → EM VOTAÇÃO
    │
    ├─► Votação (reautenticação biométrica)
    │         ├── Quorum não atingido → QUORUM NÃO ATINGIDO (nula)
    │         ├── Quorum + maioria SIM → APROVADA
    │         └── Quorum + maioria NÃO → REJEITADA
    │
    └─► Publicação do Resultado
              └── Registro imutável + estatísticas públicas + auditoria
```

---

## 13. Módulo: Dashboard

### 13.1. Dashboard do Cidadão

**Seções:**
- Propostas em andamento na sua jurisdição (coleta, debate, votação)
- Suas propostas criadas (status atual, assinaturas, votos)
- Votações que você ainda não participou (com prazo em destaque)
- Resultados recentes (últimas aprovações e rejeições)
- Estatísticas da sua participação (total de votos, propostas criadas, assinaturas)

### 13.2. Dashboard do Visitante / Público

**Seções:**
- Propostas em votação (contador regressivo)
- Propostas em coleta de assinaturas (progresso)
- Últimas leis aprovadas
- Estatísticas gerais da plataforma (por país)

### 13.3. Dashboard do Admin Master

**Seções:**
- Países ativos / pendentes de setup
- Total de cidadãos registrados (global e por país)
- Propostas ativas por país
- Alertas técnicos e de integridade da plataforma

---

## 14. Módulo: Notificações

### 14.1. Eventos de Notificação

| ID | Evento | Quem recebe |
|---|---|---|
| N01 | Proposta de sua jurisdição entrou em coleta de assinaturas | Todos os cidadãos da jurisdição |
| N02 | Proposta que você assinou atingiu o mínimo e entrou em debate | Assinantes |
| N03 | Proposta que você assinou entrou em votação | Assinantes |
| N04 | Votação na sua jurisdição encerrada — resultado disponível | Todos os cidadãos da jurisdição |
| N05 | Você ainda não votou em uma proposta ativa (lembrete) | Cidadão específico |
| N06 | Sua proposta atingiu o mínimo de assinaturas | Autor da proposta |
| N07 | Sua proposta foi arquivada por assinaturas insuficientes | Autor da proposta |
| N08 | Nova emenda proposta na sua proposta | Autor da proposta |
| N09 | Resposta ao seu comentário no debate | Cidadão comentarista |
| N10 | Resultado da proposta de meta-lei (alteração de setup) | Todos os cidadãos da jurisdição |
| N11 | Prazo de votação se encerrando em 24h | Cidadãos que ainda não votaram |
| N12 | Sua conta foi verificada com sucesso (biometria aprovada) | Cidadão |

### 14.2. Canais de Notificação

- In-app (sino de notificações no header)
- E-mail (se fornecido pelo cidadão)
- Preferências de notificação configuráveis por cidadão por evento

---

## 15. Módulo: Auditoria e Transparência

### 15.1. Registros de Auditoria

Todos os eventos críticos do sistema são registrados com: timestamp UTC, ID do cidadão (anonimizado se voto privado), ação realizada, jurisdição, hash do bloco anterior (encadeamento blockchain ou equivalente).

**Eventos auditados:**

| Evento | Dados registrados |
|---|---|
| Criação de proposta | ID cidadão, ID proposta, timestamp, hash do texto |
| Assinatura de proposta | ID cidadão (ou hash anônimo), ID proposta, timestamp |
| Emissão de voto | Voto + prova zero-knowledge (privado) ou ID cidadão + voto (público) |
| Alteração de setup | Parâmetro alterado, valor anterior, valor novo, ID da meta-lei aprovada |
| Criação de conta | Hash biométrico (não a imagem), timestamp, jurisdição |
| Acesso de auditor | ID auditor, dados acessados, timestamp |

### 15.2. Acesso Público

- Todos os resultados de votação são públicos
- Estatísticas agregadas são públicas (sem dados individuais privados)
- Código-fonte da plataforma é open-source ou sujeito a auditoria independente obrigatória
- Auditores credenciados podem verificar a integridade da contagem sem acessar votos privados individuais

---

## 16. Módulo: Integração via API

### 16.1. Endpoints Públicos (sem autenticação)

| Endpoint | Descrição |
|---|---|
| `GET /api/v1/paises` | Lista países ativos na plataforma |
| `GET /api/v1/paises/:id/propostas` | Lista propostas de uma jurisdição |
| `GET /api/v1/propostas/:id` | Detalhe de uma proposta |
| `GET /api/v1/propostas/:id/resultado` | Resultado de uma votação encerrada |
| `GET /api/v1/paises/:id/estatisticas` | Estatísticas agregadas de uma jurisdição |

### 16.2. Endpoints Autenticados (JWT + biometria)

| Endpoint | Descrição |
|---|---|
| `POST /api/v1/auth/registro` | Registro de novo cidadão |
| `POST /api/v1/auth/login` | Autenticação biométrica |
| `POST /api/v1/propostas` | Criar proposta |
| `POST /api/v1/propostas/:id/assinar` | Assinar proposta |
| `POST /api/v1/propostas/:id/votar` | Emitir voto |
| `POST /api/v1/propostas/:id/comentarios` | Comentar no debate |
| `GET /api/v1/cidadao/perfil` | Dados do cidadão autenticado |
| `PUT /api/v1/cidadao/perfil` | Atualizar perfil |

### 16.3. Integrações Externas

| Sistema | Finalidade |
|---|---|
| APIs Gov (gov.br, DNI, eID) | Validação de identidade e biometria |
| Blockchain / Ledger auditável | Imutabilidade de votos e resultados |
| Serviço de e-mail transacional | Envio de notificações |

---

## 17. Requisitos Não Funcionais

| ID | Requisito | Detalhe |
|---|---|---|
| RNF01 | Disponibilidade | 99,9% de uptime. Janelas de manutenção fora de períodos de votação ativa |
| RNF02 | Performance | Página inicial e listagem de propostas carregam em < 2s. Votação processa em < 3s |
| RNF03 | Segurança biométrica | Hash biométrico com algoritmo de última geração — nunca armazenamento de imagem bruta |
| RNF04 | Criptografia | TLS 1.3 em trânsito. AES-256 em repouso. Zero-knowledge para votos privados |
| RNF05 | Escalabilidade | Suporte a picos de votação (ex: eleições nacionais com milhões de votos simultâneos) |
| RNF06 | Auditabilidade | Blockchain ou tecnologia equivalente para imutabilidade de registros críticos |
| RNF07 | Open source | Código auditável publicamente ou por auditores independentes credenciados |
| RNF08 | Acessibilidade | WCAG 2.1 AA — suporte a leitores de tela, alto contraste, navegação por teclado |
| RNF09 | Internacionalização | Suporte a múltiplos idiomas via i18n. Datas e fusos por jurisdição |
| RNF10 | LGPD / GDPR | Conformidade com legislações de proteção de dados de cada jurisdição |
| RNF11 | Backup | Backup diário com retenção mínima de 10 anos (dados eleitorais) |
| RNF12 | Monorepo | Código-fonte organizado em monorepo (NestJS + Angular + libs compartilhadas) |
| RNF13 | Stack técnica | Backend: NestJS. Frontend: Angular. Banco: PostgreSQL. Estrutura: Monorepo |

---

## 18. Critérios de Aceite

| ID | Critério |
|---|---|
| R01 | Sistema acessível via WEB com disponibilidade em tempo real |
| R02 | Autenticação biométrica funcional: 1 pessoa = 1 conta, irreversível |
| R03 | Perfil público e privado configurável pelo cidadão |
| R04 | Voto privado e público implementados com garantias criptográficas |
| R05 | Plebiscito de configuração inicial funcional — nenhuma lei pode ser criada antes do setup |
| R06 | Parâmetros do setup só alteráveis via meta-lei com quorum/maioria especiais |
| R07 | Fluxo completo de proposta implementado com todos os 9 status |
| R08 | Coleta de assinaturas com contador em tempo real e validação biométrica |
| R09 | Período de debate com comentários, emendas e moderação por denúncia |
| R10 | Período de votação com reautenticação biométrica obrigatória |
| R11 | Cálculo automático de quorum e maioria ao encerrar votação |
| R12 | Isolamento total de dados entre jurisdições |
| R13 | Limite de propostas por cidadão por ano funcional e bloqueante |
| R14 | Dashboard funcional para cidadão, visitante e Admin Master |
| R15 | Sistema de notificações in-app e e-mail funcional com os 12 eventos definidos |
| R16 | Preferências de notificação configuráveis por cidadão |
| R17 | API pública com endpoints de listagem e resultados |
| R18 | API autenticada com endpoints de criação, assinatura e votação |
| R19 | Módulo de auditoria com registro imutável de todos os eventos críticos |
| R20 | Estatísticas agregadas sempre públicas |
| R21 | Auditores independentes podem verificar integridade sem acessar votos privados |
| R22 | Integração com ao menos uma API gov de identidade funcional |
| R23 | Backup diário implementado e auditável |
| R24 | Conformidade com WCAG 2.1 AA |
| R25 | Suporte a múltiplos idiomas (i18n) |
| R26 | Código open-source ou com auditoria independente documentada |

---

## 19. Mapa de Telas

Resumo de todas as telas do sistema organizadas por módulo, com indicação de perfil de acesso necessário.

| Módulo | Tela | Rota Frontend | Acesso |
|---|---|---|---|
| Autenticação | Login / Autenticação Biométrica | `/login` | Público |
| Autenticação | Cadastro de Cidadão | `/cadastro` | Público |
| Autenticação | Validação Biométrica | `/cadastro/biometria` | Cidadão Pendente |
| Autenticação | Troca de Senha / PIN | `/alterar-senha` | Cidadão Autenticado |
| **Admin** | **Listagem de Países** | `/admin/paises` | Admin Master |
| **Admin** | **Cadastro/Edição de País** | `/admin/paises/novo` \| `/admin/paises/:id` | Admin Master |
| **Admin** | **Gestão de Auditores** | `/admin/auditores` | Admin Master |
| Perfil | Meu Perfil | `/perfil` | Cidadão Autenticado |
| Perfil | Configurações de Privacidade | `/perfil/privacidade` | Cidadão Autenticado |
| Perfil | Preferências de Notificação | `/perfil/notificacoes` | Cidadão Autenticado |
| Propostas | Listagem de Propostas | `/propostas` | Todos |
| Propostas | Detalhe da Proposta | `/propostas/:id` | Todos |
| Propostas | Criar Proposta | `/propostas/nova` | Cidadão Autenticado |
| Propostas | Editar Proposta (rascunho) | `/propostas/:id/editar` | Autor (Cidadão Autenticado) |
| Propostas | Imprimir / Exportar Proposta | `/propostas/:id/exportar` | Todos |
| Assinaturas | Assinar Proposta | `/propostas/:id/assinar` | Cidadão Autenticado |
| Assinaturas | Lista de Assinantes Públicos | `/propostas/:id/assinantes` | Todos |
| Debate | Debate da Proposta | `/propostas/:id/debate` | Todos (comentar: Autenticado) |
| Debate | Propor Emenda | `/propostas/:id/debate/emenda` | Cidadão Autenticado |
| Votação | Votar na Proposta | `/propostas/:id/votar` | Cidadão Autenticado |
| Votação | Resultado da Votação | `/propostas/:id/resultado` | Todos |
| Meta-lei | Listagem de Meta-leis | `/metaleis` | Todos |
| Meta-lei | Criar Meta-lei (alteração de setup) | `/metaleis/nova` | Cidadão Autenticado |
| Meta-lei | Detalhe / Votação de Meta-lei | `/metaleis/:id` | Todos |
| Setup | Plebiscito de Configuração Inicial | `/setup/:paisId` | Cidadão Autenticado |
| Setup | Histórico do Setup | `/setup/:paisId/historico` | Todos |
| Dashboard | Dashboard Principal | `/dashboard` | Todos |
| Notificações | Central de Notificações | `/notificacoes` | Cidadão Autenticado |
| Auditoria | Log de Auditoria Pública | `/auditoria` | Todos |
| Auditoria | Auditoria Avançada | `/auditoria/avancada` | Auditor Independente, Admin Master |
| Estatísticas | Estatísticas Públicas por País | `/estatisticas/:paisId` | Todos |

> **Total:** 31 telas distribuídas em 13 módulos. Todas as telas de interação (votar, assinar, criar proposta) exigem autenticação JWT + revalidação biométrica. Telas de visualização (propostas, resultados, estatísticas) são públicas para visitantes. O Admin Master opera em escopo global da plataforma. Auditores Independentes têm acesso somente leitura a dados agregados e provas criptográficas.

---

*Documento gerado em: abril de 2026*
*Versão: 1.0.0*
*Sistema: Democracy Without Candidates*
