# Democracy Without Candidates
## Product Requirements Document (PRD)

> **Versão:** 1.2.0 | **Atualização:** 2026-10-12 | **Status:** Especificação; aplicação não implementada

Arquitetura aprovada no [plano SDD](../development/sdd-plan.md). Os critérios abaixo são requisitos futuros, sem evidência de implementação. A revisão substitui a stack anterior e explicita decisões abertas; não aprova novas regras eleitorais nem um protocolo criptográfico.

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
9. [Interfaces e capacidades de acesso](#9-interfaces-e-capacidades-de-acesso)
10. [Requisitos Não Funcionais](#10-requisitos-não-funcionais)
11. [Mapa de Telas (31 Telas)](#11-mapa-de-telas-31-telas)
12. [Critérios de Aceite](#12-critérios-de-aceite)
13. [Decisões de produto pendentes](#13-decisões-de-produto-pendentes)

---

## 1. Visão Geral do Produto

O **Democracy Without Candidates** é uma plataforma web multi-país de democracia direta digital que permite a qualquer cidadão autenticado propor, debater e votar leis e decisões coletivas — sem intermediários, representantes ou candidatos. O sistema elimina a figura do candidato e do partido, devolvendo ao cidadão o poder de legislar diretamente.

O produto prevê autenticação biométrica facial com segundo fator TOTP, privacidade configurável e regras definidas por plebiscito inicial (setup). **Uma pessoa = uma conta e um voto por eleição** é requisito a demonstrar por prova de identidade, elegibilidade e prevenção de duplicidade; biometria e 2FA, isoladamente, não comprovam essa garantia. Provedor real e protocolo de voto privado permanecem pendentes.

### 1.1 Pilares do Sistema

| Pilar | Descrição |
|-------|-----------|
| **1 pessoa = 1 voto** | Prova de identidade e unicidade de participação verificadas, inclusive entre modos público/privado |
| **Soberania do setup** | Cada jurisdição define suas próprias regras via votação direta |
| **Privacidade por padrão** | Voto secreto como padrão, com opt-in público |
| **Transparência** | Resultados, estatísticas e evidências públicas sem divulgar dados pessoais ou comprometer votos privados |
| **Código auditável** | Plataforma open-source ou com auditoria independente obrigatória |

### 1.2 Stack Técnica aprovada

| Camada | Tecnologia | Detalhes |
| --- | --- | --- |
| Aplicação | Rust | Monolito modular, inicialmente um pacote Cargo |
| Frontend | Leptos SSR + hidratação WASM | Responsivo, mobile-first; PWA continua requisito futuro |
| Servidor | Axum integrado ao Leptos + Tokio | Server functions como interface inicial; regras e autorização no servidor |
| Banco de Dados | TursoDB local pela crate `turso` | Um processo proprietário do banco persistente |
| Países | Implantação independente | Subdomínio, VPS, banco e configuração por país; início com um país |
| Identidade | Provider a definir + segundo fator TOTP | Mock só para desenvolvimento/testes, rejeitado em produção |
| Permissões | RBAC + políticas por recurso/jurisdição | Nenhuma autorização definitiva no navegador |
| Verificação | Rust, banco real, navegador e carga em release | Estratégia detalhada no plano SDD |
| Segurança eleitoral | Protocolo a selecionar e validar | Não há implementação de sigilo, prova ou custódia de chaves |

A stack anterior foi substituída, com histórico preservado no Git. A decisão Rust não elimina HTML/CSS nem o JavaScript gerado para carregar o WASM.

### 1.3 Tipos de Proposta

- Lei Normal
- Lei Urgente
- Proposta de Alteração do Setup (Meta-lei)
- Plebiscito de Configuração Inicial

---

## 2. Arquitetura Multi-País

### 2.1 Países e hierarquia territorial

Cada país tem sua própria instância e configuração, sem banco global exigido pelo runtime inicial. Dentro de cada país existem níveis nacional, estadual e municipal.

```text
Implantação do país (subdomínio + VPS + aplicação + banco)
└── Jurisdição nacional
    └── Jurisdições estaduais
        └── Jurisdições municipais
```

O papel Admin Master e os fluxos de administração global permanecem previstos para expansão, mas sua autoridade e integração entre instâncias precisam ser revistas. Não pressupõem acesso automático aos bancos de outros países.

### 2.2 Regras de Isolamento

| Regra | Descrição |
|-------|-----------|
| **Isolamento por País** | Dados, votos e propostas de um país nunca são misturados com outro |
| **Isolamento por Nível** | Propostas municipais não interferem em estaduais ou nacionais |
| **Cidadão por Jurisdição** | Um cidadão é registrado em uma jurisdição por vez |
| **Setup por País** | Cada país tem seu próprio conjunto de regras definido por plebiscito |
| **Imutabilidade de Votos** | Votos emitidos não podem ser alterados, apenas auditados |

### 2.3 Parâmetros Padrão Sugeridos

Estes valores são propostas de configuração, não regras de produção já aprovadas. A expressão “60% + 1”, arredondamentos, eleitorado de referência e casos de fronteira precisam de definição explícita antes dos testes eleitorais.

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

O requisito de produto é reconhecimento facial e segundo fator TOTP. Provedor, prova de identidade, recuperação e integração real permanecem abertos. A referência anterior a `auth-face` não aprova uma implementação ou suas garantias. Ver [identidade e autorização](../architecture/auth-rbac-flow.md).

### 3.1 Fluxo de Autenticação a especificar

1. Identificar a conta e validar credenciais conforme contrato ainda a decidir.
2. Verificar a identidade pelo provider real, com liveness e falhas definidos.
3. Validar segundo fator TOTP conforme política de segurança e recuperação.
4. Estabelecer sessão e obter contexto de autorização no servidor.

Senha versus fluxo sem senha, formato de sessão e eventual uso de JWT não estão decididos. Cadastro sintético valida fluxos de desenvolvimento, sem demonstrar biometria ou unicidade real.

### 3.2 Reautenticação para Ações Críticas

As seguintes ações exigem revalidação biométrica completa (face + 2FA):

- Emissão de voto
- Assinatura de proposta
- Criação de proposta
- Alteração de dados de perfil sensíveis e privacidade

O guia anterior também inclui participação em alteração de setup. Especificar validade, escopo da ação/recurso e prevenção de replay da reautenticação antes de implementar esses fluxos.

### 3.3 Segurança Biométrica

| Aspecto | Requisito e limite atual |
| --- | --- |
| Armazenamento | Não persistir imagem bruta; template/referência protegida depende do provider |
| Transmissão | Meta de TLS 1.3; configuração e evidência ainda ausentes |
| Repouso | Meta de AES-256 para dados sensíveis, com gestão de chaves a especificar; sua menção não demonstra criptografia implementada |
| Unicidade | Definir prova documental e prevenção de duplicidade/residência; igualdade de hash biométrico não especifica matching |

### 3.4 Perfil e privacidade

Perfil público pode exibir nome real ou pseudônimo verificado, foto, bio, histórico de propostas e votos emitidos como públicos. Perfil privado não divulga documento ou dados pessoais aos demais usuários; sua apresentação deve seguir a preferência configurada.

O cidadão pode alterar a preferência do perfil; votos já emitidos mantêm a visibilidade escolhida na emissão. Definir quais históricos podem ser publicados sem permitir correlação com voto privado. Estatísticas por região e demografia, quando autorizadas, precisam de regras de agregação/divulgação antes de implementar.

---

## 4. Módulo: RBAC — Controle de Acesso por Papel

| Papel (Role) | Descrição | Permissões Principais |
|-------------|-----------|----------------------|
| **Visitante** | Não autenticado | Leitura pública (propostas, resultados, estatísticas) |
| **Cidadão Pendente** | Cadastrado, biometria pendente | Completar cadastro |
| **Cidadão** | Autenticado e validado | Propor, assinar, votar, comentar, perfil |
| **Auditor Independente** | Credenciado para auditoria | Logs avançados, provas criptográficas (somente leitura) |
| **Admin País** | Administrador da instância/jurisdição | Operação autorizada; regras eleitorais só mudam por meta-lei aprovada |
| **Admin Master** | Funções futuras de plataforma | Cadastro de países/auditores e escopo global ainda a revisar; sem acesso implícito entre instâncias |

---

## 5. Módulo: Propostas e Leis

### 5.1 Status de uma Proposta

| Status | Descrição |
|--------|-----------|
| **Rascunho** | Criada pelo autor, não publicada |
| **Coleta de Assinaturas** | Publicada, coletando assinaturas |
| **Em Debate** | Assinaturas atingidas, período de debate ativo |
| **Em Votação** | Fase anterior encerrada conforme o tipo de proposta, votação aberta |
| **Aprovada** | Quórum e maioria atingidos |
| **Rejeitada** | Maioria não atingida |
| **Quórum Não Atingido** | Votação encerrada sem quórum suficiente |
| **Expirada** | Prazo de assinaturas esgotado sem atingir meta |
| **Arquivada** | Removida de circulação ativa |

Empate, arquivamento, edição após publicação, retirada de assinatura e efeito de emendas precisam de transições explícitas. As assinaturas devem referenciar o texto apoiado conforme regra a definir.

### 5.2 Limites por Cidadão por Ano

- Propostas normais: **5 por ano**
- Propostas urgentes: **1 por ano**
- Limite bloqueante — sistema deve impedir a ação que consumir a quota após atingir o limite

Os valores vêm da configuração sugerida. Definir ano civil/fuso, momento de consumo (rascunho ou publicação), cancelamentos e concorrência. Leis não podem ser criadas antes da adoção do setup; plebiscito inicial tem seu próprio bootstrap ainda a especificar.

### 5.3 Fluxos sugeridos por tipo

Lei normal: coleta de assinaturas → debate → votação. Lei urgente: coleta → votação, conforme os prazos sugeridos da seção 2.3. Meta-lei usa votação especial de setup, sem se confundir com lei comum.

Especificar transições e retomada após restart, incluindo expiração da coleta e disputa entre envio/encerramento. Bootstrap do plebiscito inicial e requisitos de uma meta-lei ainda precisam de regras completas; este fluxo não resolve essas pendências.

---

## 6. Módulo: Votação

### 6.1 Opções de Voto

- ✅ **SIM**
- ❌ **NÃO**
- ⚪ **ABSTENÇÃO**

### 6.2 Privacidade do Voto

- **Padrão:** Voto privado, com protocolo eleitoral a selecionar e validar; ZKP é requisito/proposta de mecanismo, não implementação existente
- **Opt-in:** Voto público (vinculado ao cidadão com consentimento)
- Configurável por cidadão a cada votação

O requisito é impedir voto duplicado também ao alternar os modos público e privado. Alterar a preferência do perfil não muda a visibilidade de votos já emitidos. Recibos, reautenticação, logs e estatísticas precisam ser avaliados para não criar vínculos que contradigam o sigilo prometido.

### 6.3 Cálculo de Resultado

| Verificação | Critério |
|------------|---------|
| **Quórum** | % de eleitores aptos que votaram >= mínimo configurado |
| **Maioria** | % de votos válidos (SIM vs NÃO, excluindo abstenções) |
| **Resultado** | Aprovada (quórum + maioria) \| Rejeitada \| Quórum não atingido |

Abstenções são excluídas da maioria neste requisito. Ainda é necessário definir snapshot do eleitorado, tratamento no quórum, arredondamentos, empate, zero votos válidos e fórmulas por tipo de eleição. Fixar versão do setup e vigência de alterações para não mudar regras de eleição em curso. Voto aceito é imutável no produto pretendido, mas persistência e integridade ainda exigem implementação e validação.

---

## 7. Módulo: Dashboard

| Perfil | Conteúdo Exibido |
|--------|-----------------|
| **Visitante** | Votações ativas, propostas públicas, estatísticas da jurisdição |
| **Cidadão** | Tudo do visitante + Minhas propostas, Minhas assinaturas, Notificações |
| **Admin Master** | Estatísticas globais, saúde e gestão de países como escopo futuro a revisar |

---

## 8. Módulo: Auditoria e Transparência

| Evento | Evidência pretendida e decisão pendente |
| --- | --- |
| Criação de proposta | Autoria autorizada, proposta, versão/hash do texto e data |
| Assinatura de proposta | Elegibilidade e texto apoiado, respeitando a política de divulgação |
| Emissão de voto | Evidência definida pelo protocolo; sem associação indevida entre identidade e cédula privada |
| Alteração de setup | Valores/versões, vigência e meta-lei aprovada |
| Criação de conta | Estado da validação e evidência mínima, sem publicar documento ou biometria |
| Acesso de auditor | Auditor, escopo autorizado e acesso realizado |

Definir modelo de ameaça, evidências públicas/restritas, retenção e verificação independente. Não registrar automaticamente identidade, IP ou vínculo de reautenticação junto ao voto privado. Hash chain e proibição de UPDATE/DELETE não comprovam por si resistência à adulteração pelo operador. Blockchain ou equivalente permanece uma proposta a avaliar, sem serviço adicional aprovado.

---

## 9. Interfaces e capacidades de acesso

A interface inicial será composta por páginas SSR e server functions Leptos, integradas ao Axum. Cada endpoint protegido obtém sessão no servidor e delega à aplicação. Consumidores e contrato de uma API externa permanecem a especificar; uma API REST completa paralela não integra a fundação.

| Público | Capacidades de produto |
| --- | --- |
| Visitante | Listar propostas, ler detalhes/resultados e estatísticas públicas da instância |
| Cidadão autenticado e elegível | Cadastro/login conforme contrato, perfil, criação de proposta, assinatura, voto e comentários |
| Administração/auditoria | Operações autorizadas por papel, recurso e jurisdição, sem edição direta de regras eleitorais |
| Plataforma global futura | Catálogo de países e operação entre instâncias ainda a revisar |

Os caminhos `/api/v1/...` da proposta anterior não são endpoints existentes ou contrato aprovado para o scaffold. Os critérios R18/R19 preservam leitura pública e operações autenticadas; R18/R19 não afirmam entrega de API REST externa. Se um consumidor externo for necessário, decidir seu contrato e validar autorização/compatibilidade antes de implementá-lo.

---

## 10. Requisitos Não Funcionais

| ID | Requisito | Detalhe |
|----|-----------|---------|
| RNF01 | Disponibilidade | Meta de 99,9%; operação, manutenção e recuperação durante votação a especificar/validar |
| RNF02 | Performance | Metas de listagem < 2s e aceitação de voto < 3s; interpretação de trabalho p95 no plano SDD, carga-alvo ainda aberta |
| RNF03 | Segurança biométrica | Não persistir imagem bruta; provider e template/referência protegida a especificar |
| RNF04 | Criptografia | Metas TLS 1.3, AES-256 em repouso e voto privado verificável; protocolo, chaves e evidência pendentes |
| RNF05 | Escalabilidade | Aspiração de picos com milhões de votos; volume simultâneo, carga e recursos a definir, sem capacidade demonstrada |
| RNF06 | Auditabilidade | Evidência verificável de integridade; mecanismo, ameaça e eventual blockchain/equivalente a decidir |
| RNF07 | Open source | Código auditável publicamente ou por auditores credenciados |
| RNF08 | Acessibilidade | WCAG 2.1 AA — leitores de tela, alto contraste, teclado |
| RNF09 | Internacionalização | i18n — múltiplos idiomas, datas e fusos por jurisdição |
| RNF10 | Proteção de dados | Requisito de adequação ao país; tratamento, retenção e direitos a especificar, sem alegar conformidade demonstrada |
| RNF11 | Backup | Meta inicial diária e retenção de 10 anos; política por categoria, RPO/RTO e restauração ainda a decidir/validar |
| RNF12 | Organização | Monolito modular Rust, inicialmente um pacote de aplicação |
| RNF13 | Stack técnica | Rust, Leptos SSR/hidratação WASM, Axum/Tokio e TursoDB local |
| RNF14 | PWA | Frontend instalável como app no celular e desktop |
| RNF15 | Ambiente reproduzível | Toolchain/lockfile e harness local/CI; containerização não é pré-requisito aprovado |

---

## 11. Mapa de Telas (31 Telas)

Mapa pretendido, não telas implementadas. Rotas de gestão global e seleção entre países são escopo futuro a revisar. Troca de senha/PIN depende da decisão sobre credenciais; não resolve o login por conta própria.

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

Todos estão pendentes, sem testes executados. Os IDs R01–R26 foram mantidos. Critérios técnicos R18/R19/R25 foram reconciliados com as interfaces e o ambiente aprovados, sem exigir a stack anterior. Detalhar casos positivos/negativos e evidência por feature antes de implementar.

| ID | Critério |
|----|---------|
| R01 | Sistema acessível via WEB (PWA) com disponibilidade em tempo real |
| R02 | Prova de identidade com face + 2FA e prevenção de duplicidade demonstradas; mock não satisfaz provider real |
| R03 | RBAC implementado com todos os 6 papéis definidos |
| R04 | Perfil público e privado configurável pelo cidadão |
| R05 | Voto privado e público com protocolo/garantias validados, incluindo unicidade entre modos e verificação do recibo |
| R06 | Plebiscito de configuração inicial — nenhuma lei criada antes do setup |
| R07 | Parâmetros do setup só alteráveis via meta-lei com quórum/maioria especiais |
| R08 | Fluxo completo de proposta com todos os 9 status |
| R09 | Coleta de assinaturas com contador em tempo real e validação biométrica |
| R10 | Período de debate com comentários, emendas e moderação |
| R11 | Período de votação com reautenticação biométrica obrigatória |
| R12 | Encerramento/apuração aplicam fórmulas e versões aprovadas, com casos de empate/abstenção/concorrência testados |
| R13 | Isolamento total de dados entre jurisdições |
| R14 | Limite de propostas por cidadão por ano funcional e bloqueante |
| R15 | Dashboard funcional para cidadão, visitante e Admin Master |
| R16 | Sistema de notificações in-app e e-mail funcional |
| R17 | Preferências de notificação configuráveis por cidadão |
| R18 | Interface pública de listagem e resultados funcional; contrato externo separado somente quando definido |
| R19 | Server functions autenticadas de criação, assinatura e voto aplicam autorização e invariantes no servidor |
| R20 | Eventos críticos têm integridade verificável conforme modelo de ameaça e mecanismo aprovados |
| R21 | Auditores independentes verificam integridade sem acessar votos privados |
| R22 | Backup diário auditável e restauração testada, conforme retenção e RPO/RTO aprovados |
| R23 | Conformidade com WCAG 2.1 AA |
| R24 | Suporte a múltiplos idiomas (i18n) |
| R25 | Ambiente local/CI reproduzível compila SSR/WASM e executa os checks definidos, sem resetar dados persistentes |
| R26 | Frontend instalável como PWA no celular e desktop |

---

## 13. Decisões de produto pendentes

| Tema | Decisão necessária antes da feature afetada |
| --- | --- |
| País e implantação | Primeiro país/domínio, locale, VPS e carga-alvo |
| Identidade e sessão | Provider, prova documental, liveness/matching, credenciais, recuperação e expiração/revogação |
| Eleições | Eleitorado/snapshot, residência, fórmulas/arredondamento, abstenção, empate e zero votos válidos |
| Setup | Quem inicia, regras do primeiro plebiscito, falha de quórum, autoridade operacional e vigência de meta-leis |
| Propostas/debate | Consumo de quota, retirada de assinatura, edição/emenda após apoio, moderação e denúncia sem censura prévia arbitrária |
| Sigilo e auditoria | Protocolo, ameaça, chaves, unicidade entre modos, provas/recibos e correlação de logs |
| Operação e dados | Retenção por categoria, RPO/RTO, proteção de backup, atualização e restauração durante votação |
| Interface e expansão | PWA/cache/offline, atendimento físico, notificações push, API externa e administração global |

Essas decisões não bloqueiam a fundação técnica sintética, mas impedem prometer os respectivos critérios de produção. Não resolvê-las por suposição: apresentar opções ao usuário. A ordem de trabalho está no [plano SDD](../development/sdd-plan.md); requisitos e tarefas da primeira fatia estão no [guia de fundação](../architecture/foundation-setup.md).

*Democracy Without Candidates — PRD v1.2.0 — 2026-10-12*
