# Democracy Without Candidates
## Product Requirements Document (PRD)

> **Versão:** 1.4.0 | **Atualização:** 2026-10-25 | **Status:** Implementação demonstrativa com dados sintéticos; sem validade eleitoral

Arquitetura aprovada no [plano SDD](../development/sdd-plan.md). O estado de cada critério está na [seção 12](#12-critérios-de-aceite). As [decisões pendentes](#13-decisões-de-produto-pendentes) precisam ser resolvidas antes das features afetadas.

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

O login identifica a conta por e-mail; cadastro exige e-mail, senha, nome e endereço, sem CPF ou outro documento nacional. Login exige senha, autenticação biométrica facial e TOTP. O produto prevê privacidade configurável e regras definidas por plebiscito inicial (setup). **Uma pessoa = uma conta e um voto por eleição** é requisito a demonstrar por identidade, elegibilidade e prevenção de duplicidade; e-mail, biometria e 2FA, isoladamente, não comprovam essa garantia. O motor facial Python do auth-face foi escolhido para integração provisória, ainda sem validação. Protocolo de voto privado permanece pendente.

### 1.1 Pilares do Sistema

| Pilar | Descrição |
|-------|-----------|
| **1 pessoa = 1 voto** | Prova de identidade e unicidade de participação verificadas, inclusive entre modos público/privado |
| **Soberania do setup** | Regras definidas por votação direta; proposta de setup por jurisdição com herança nacional, para revisão na seção 13 |
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
| Identidade | Motor facial Python do auth-face + segundo fator TOTP previsto | Serviço privado na mesma VPS; Rust mantém contas/sessões; integração, liveness e TOTP ainda não validados |
| Permissões | RBAC + políticas por recurso/jurisdição | Nenhuma autorização definitiva no navegador |
| Verificação | Rust, banco real, navegador e carga em release | Estratégia detalhada no plano SDD |
| Segurança eleitoral | Protocolo a selecionar e validar | Não há implementação de sigilo, prova ou custódia de chaves |

A decisão Rust inclui HTML/CSS e JavaScript gerado para carregar o WASM.

### 1.3 Tipos de Proposta

- Lei Normal
- Lei Urgente
- Proposta de Alteração do Setup (Meta-lei)
- Plebiscito de Configuração Inicial

---

## 2. Arquitetura Multi-País

### 2.1 Países e hierarquia territorial

O país inicial é o Brasil. A mesma aplicação deve atender outros países por configuração própria de implantação, idioma e organização territorial, sem regras brasileiras fixadas no código de domínio. O cadastro usa e-mail sem documento nacional. A elegibilidade e as regras eleitorais de cada país exigem especificação e validação próprias.

Cada país tem sua própria instância e configuração, sem banco global exigido pelo runtime inicial. No Brasil, o modelo prevê níveis nacional, estadual e municipal; a hierarquia de outros países depende da configuração e especificação da implantação.

```text
Implantação inicial: Brasil (subdomínio + VPS + aplicação + banco)
└── Jurisdição nacional
    └── Jurisdições estaduais
        └── Jurisdições municipais
```

O papel Admin Master e os fluxos de administração global permanecem previstos para expansão, mas sua autoridade e integração entre instâncias precisam ser revistas. Não pressupõem acesso automático aos bancos de outros países.

O uso é exclusivamente online. Votação, coleta de assinaturas e atendimento eleitoral em meio físico não integram o escopo. Instalabilidade como PWA não inclui modo offline.

### 2.2 Regras de Isolamento

| Regra | Descrição |
|-------|-----------|
| **Isolamento por País** | Dados, votos e propostas de um país nunca são misturados com outro |
| **Isolamento por Nível** | Propostas municipais não interferem em estaduais ou nacionais |
| **Cidadão por Jurisdição** | Endereço inicial define cidade, estado e país de participação; alteração de endereço bloqueada nesta versão. Validação e demais critérios de elegibilidade ainda pendentes |
| **Setup** | Regras por plebiscito; proposta de herança nacional e adoção por jurisdição, para revisão no SDD |
| **Imutabilidade de Votos** | Votos emitidos não podem ser alterados, apenas auditados |

O alcance do isolamento e participação territorial tem uma proposta para revisão na [seção 13](#13-decisões-de-produto-pendentes): leitura pública entre territórios e participação ligada ao endereço inicial. A hierarquia não resolve essas regras por si só.

### 2.3 Parâmetros Padrão Sugeridos

Estes valores são propostas de configuração, não regras de produção já aprovadas. Fórmulas, arredondamento, snapshot e casos de fronteira têm uma [proposta do agente para revisão](../development/sdd-plan.md#regras-eleitorais-e-de-propostas-para-revisão).

| Parâmetro | Valor Padrão |
|-----------|-------------|
| Assinaturas — proposta nacional | 0,5% do eleitorado |
| Assinaturas — proposta estadual | 1% do eleitorado estadual |
| Assinaturas — proposta municipal | 2% do eleitorado municipal |
| Quórum mínimo — leis normais | 25% dos eleitores aptos |
| Quórum mínimo — leis urgentes | 20% dos eleitores aptos |
| Quórum — configuração inicial | 40% dos eleitores aptos |
| Maioria — configuração inicial | Mais de 60% dos votos válidos; floor(0,60 × válidos)+1 — proposta para revisão |
| Quórum — alteração de setup | 35% dos eleitores aptos |
| Maioria — alteração de setup | Mais de 60% dos votos válidos; floor(0,60 × válidos)+1 — proposta para revisão |
| Prazo coleta assinaturas — normal | 90 dias |
| Prazo debate — normal | 15 dias |
| Prazo votação — normal | 30 dias |
| Prazo coleta assinaturas — urgente | 15 dias |
| Prazo votação — urgente | 7 dias |
| Limite propostas normais/cidadão/ano | 5 |
| Limite propostas urgentes/cidadão/ano | 1 |

---

### 2.4 Feature flags por instância

Um único código atende todas as implantações. Funcionalidades podem ser ativadas/desativadas por feature flags armazenadas no banco da instância. As flags são carregadas na inicialização; mudanças no banco só entram em vigor após reiniciar a aplicação. O servidor aplica o estado efetivo aos casos de uso e informa a disponibilidade à interface, mantendo autorização e regras de domínio. Flags não substituem regras aprovadas por plebiscito. Ver [contrato do servidor](../architecture/backend-architecture.md#feature-flags-por-instância).

---

### 2.5 Endereço por CEP

No Brasil, o cadastro consulta BrasilAPI ao preencher CEP e completa estado, cidade, bairro e logradouro. O provedor é habilitado/selecionado por feature flag no banco da instância, permitindo outro adaptador em outros países. O servidor valida o resultado antes de persistir, mantendo os nomes do retorno conforme o [modelo de endereço](../architecture/database-structure.md#endereço-e-brasilapi).

País vem da configuração da VPS. O endereço usa apenas os campos retornados pelo BrasilAPI, sem número ou complemento nesta versão. Consulta postal não comprova residência. Depois do primeiro cadastro, endereço e vínculo territorial permanecem bloqueados para alteração.

---

## 3. Módulo: Autenticação Biométrica

O requisito de produto é reconhecimento facial e segundo fator TOTP. O aproveitamento do motor Python do auth-face foi aprovado, sem integração executada. As demais decisões sobre identidade e sessão estão na [seção 13](#13-decisões-de-produto-pendentes); fluxos e critérios técnicos em [identidade e autorização](../architecture/auth-rbac-flow.md).

### 3.1 Fluxo de Autenticação a especificar

1. Identificar a conta pelo e-mail e validar a senha no servidor.
2. Verificar a identidade pelo provider real, com liveness e falhas definidos.
3. Validar segundo fator TOTP conforme política de segurança e recuperação.
4. Estabelecer sessão e obter contexto de autorização no servidor.

Login por e-mail + senha, rosto e TOTP está definido. Nesta versão, a ativação não exige confirmação do e-mail; o e-mail informado não deve ser apresentado como verificado. Não oferecer “manter conectado” ou persistência de login entre usos. Sessão expira após 15 minutos sem atividade. Sessão no TursoDB da instância com cookie seguro está definida; reinício preserva última atividade/expiração/revogação e não concede um novo prazo. Definição/renovação de atividade, limite absoluto, armazenamento seguro de senha, política/troca de senha e detalhes de segurança da recuperação ainda precisam de especificação. Recuperação de senha usa rosto + TOTP, sem envio de e-mail. Perda do TOTP usa códigos de recuperação de uso único fornecidos no cadastro, mantendo senha e rosto; detalhes de geração e consumo estão no guia de identidade. Cadastro sintético valida fluxos de desenvolvimento, sem demonstrar biometria ou unicidade real.

### 3.2 Reautenticação para Ações Críticas

As seguintes ações exigem revalidação biométrica completa (face + 2FA):

- Emissão de voto
- Assinatura de proposta
- Criação de proposta
- Alteração de dados de perfil sensíveis e privacidade

Participação em alteração de setup também exige a reautenticação prevista no [guia de identidade](../architecture/auth-rbac-flow.md#reautenticação-e-autorização-no-servidor). A proposta do SDD vincula reautenticação à sessão/ação/recurso por cinco minutos, com consumo único; exige validação antes de implementar estes fluxos.

### 3.3 Segurança Biométrica

| Aspecto | Requisito e limite atual |
| --- | --- |
| Armazenamento | Não persistir imagem bruta; template/referência protegida depende do provider |
| Transmissão | Meta de TLS 1.3; configuração e evidência ainda ausentes |
| Repouso | Meta de AES-256 para dados sensíveis, com gestão de chaves a especificar; sua menção não demonstra criptografia implementada |
| Unicidade | Sem documento nacional; prevenção de duplicidade/elegibilidade ainda a definir. Unicidade de e-mail ou igualdade de hash biométrico não comprova uma conta por pessoa |

### 3.4 Perfil e privacidade

Perfil público pode exibir nome real ou pseudônimo verificado, foto, bio, histórico de propostas e votos emitidos como públicos. Perfil privado não divulga e-mail ou dados pessoais aos demais usuários; sua apresentação deve seguir a preferência configurada.

Endereço residencial não é dado público, independentemente da visibilidade do perfil. Após gravação inicial do cadastro, não pode ser alterado nesta versão; exibir ao titular somente para leitura.

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

As [regras propostas para revisão](../development/sdd-plan.md#regras-eleitorais-e-de-propostas-para-revisão) definem empate, arquivamento, texto publicado imutável e retirada/reassinatura na coleta. Assinaturas referenciam a versão apoiada; emendas são sugestões, sem mudar texto já apoiado.

### 5.2 Limites por Cidadão por Ano

- Propostas normais: **5 por ano**
- Propostas urgentes: **1 por ano**
- Limite bloqueante — sistema deve impedir a ação que consumir a quota após atingir o limite

Os valores vêm da configuração sugerida. Proposta para revisão: quota por ano civil no fuso da instância, consumida atomicamente na publicação; rascunho não consome e cancelamento não devolve. Leis não podem ser criadas antes da adoção do setup; plebiscito inicial tem seu próprio bootstrap ainda a especificar.

### 5.3 Fluxos sugeridos por tipo

Lei normal: coleta de assinaturas → debate → votação. Lei urgente: coleta → votação, conforme os prazos sugeridos da seção 2.3. Meta-lei usa votação especial de setup, sem se confundir com lei comum.

Especificar transições e retomada após restart, incluindo expiração da coleta e disputa entre envio/encerramento. Bootstrap do plebiscito inicial e meta-leis seguem as propostas do SDD, ainda para revisão, com vigência apenas em eleições futuras.

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

Abstenções são excluídas da maioria neste requisito. Snapshot, quórum com abstenção, arredondamentos e rejeição por empate/zero votos válidos têm propostas para revisão no SDD. Fixar versão do setup e vigência de alterações para não mudar regras de eleição em curso. Voto aceito é imutável no produto pretendido, mas persistência e integridade ainda exigem implementação e validação.

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
| Criação de conta | Estado da validação e evidência mínima, sem publicar e-mail ou biometria |
| Acesso de auditor | Auditor, escopo autorizado e acesso realizado |

As propostas de auditoria/retenção do SDD exigem modelo de ameaça e verificação independente antes de produção. Não registrar automaticamente identidade, IP ou vínculo de reautenticação junto ao voto privado. Hash chain e proibição de UPDATE/DELETE não comprovam por si resistência à adulteração pelo operador. A proposta inicial dispensa blockchain e exige checkpoints externos verificáveis; não há serviço adicional aprovado.

---

## 9. Interfaces e capacidades de acesso

A interface inicial será composta por páginas SSR e server functions Leptos, integradas ao Axum. Cada endpoint protegido obtém sessão no servidor e delega à aplicação. Consumidores e contrato de uma API externa permanecem a especificar; uma API REST completa paralela não integra a fundação.

| Público | Capacidades de produto |
| --- | --- |
| Visitante | Listar propostas, ler detalhes/resultados e estatísticas públicas da instância |
| Cidadão autenticado e elegível | Cadastro/login conforme contrato, perfil, criação de proposta, assinatura, voto e comentários |
| Administração/auditoria | Operações autorizadas por papel, recurso e jurisdição, sem edição direta de regras eleitorais |
| Plataforma global futura | Catálogo de países e operação entre instâncias ainda a revisar |

R18/R19 abrangem leitura pública e operações autenticadas pela interface inicial. Uma API externa depende de contrato e consumidores definidos, conforme a [seção 13](#13-decisões-de-produto-pendentes).

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
| RNF11 | Backup | Proposta: completo diário e recuperação horária, 7 diários/4 semanais; RPO 1h/RTO 4h a validar. Retenção eleitoral pública minimizada por 10 anos, sem aplicar a todos os backups/dados pessoais |
| RNF12 | Organização | Monolito modular Rust, inicialmente um pacote de aplicação |
| RNF13 | Stack técnica | Rust, Leptos SSR/hidratação WASM, Axum/Tokio e TursoDB local |
| RNF14 | PWA | Frontend instalável como app no celular e desktop, com uso exclusivamente online |
| RNF15 | Ambiente reproduzível | Toolchain/lockfile e harness local/CI; containerização não é pré-requisito aprovado |

---

## 11. Mapa de Telas (31 Telas)

Telas implementadas, exceto a gestão global de países. Desvios: setup em `/setup` e `/setup/historico` e estatísticas em `/estatisticas` (uma instância por país); assinatura feita na página da proposta, sem `/assinar`; exportação como download em `/propostas/:id/exportar`. Rotas de gestão global e seleção entre países são escopo futuro a revisar. Critérios de recuperação constam no guia de identidade e nas propostas do SDD, ainda sem implementação; PIN não é credencial aprovada.

| Módulo | Tela | Rota | Acesso |
|--------|------|------|--------|
| Autenticação | Login / Auth Biométrica | `/login` | Público |
| Autenticação | Cadastro de Cidadão | `/cadastro` | Público |
| Autenticação | Validação Biométrica | `/cadastro/biometria` | Cidadão Pendente |
| Autenticação | Troca de Senha | `/alterar-senha` | Cidadão Autenticado |
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

Os IDs R01–R29 são estáveis. Estado baseado na evidência registrada na [fundação](../architecture/foundation-setup.md#estado-da-evidência); "implementado" com identidades sintéticas não significa pronto para uso eleitoral. R03/R15 preservam a administração global como expansão futura, sem torná-la requisito da instância inicial. Detalhar casos positivos/negativos e evidência por feature antes de implementar.

| ID | Critério | Estado em 2026-10-25 |
|----|---------|--------|
| R01 | Sistema acessível via WEB (PWA) com disponibilidade em tempo real | Parcial: web online e PWA; disponibilidade não medida |
| R02 | Prova de identidade com face + 2FA e prevenção de duplicidade demonstradas; mock não satisfaz provider real | Não atendido: só provider sintético |
| R03 | RBAC com os papéis da instância e políticas aprovadas; Admin Master depende da definição da expansão global | Parcial: papéis da instância; Admin Master futuro |
| R04 | Perfil público e privado configurável pelo cidadão | Implementado |
| R05 | Voto privado e público com protocolo/garantias validados, incluindo unicidade entre modos e verificação do recibo | Não atendido: só voto público demonstrativo |
| R06 | Plebiscito de configuração inicial — nenhuma lei criada antes do setup | Implementado e testado |
| R07 | Parâmetros do setup só alteráveis via meta-lei com quórum/maioria especiais | Implementado e testado |
| R08 | Fluxo completo de proposta com todos os 9 status | Implementado |
| R09 | Coleta de assinaturas com contador em tempo real e validação biométrica | Implementado; contador atualiza após ação, sem push |
| R10 | Período de debate com comentários, emendas e moderação | Implementado |
| R11 | Período de votação com reautenticação biométrica obrigatória | Implementado com reautenticação sintética |
| R12 | Encerramento/apuração aplicam fórmulas e versões aprovadas, com casos de empate/abstenção/concorrência testados | Implementado com regras propostas; empate/abstenção/concorrência testados |
| R13 | Isolamento entre países e políticas territoriais verificadas; leitura pública e participação entre níveis dependem das regras aprovadas na seção 13 | Parcial: território testado; regras em revisão |
| R14 | Limite de propostas por cidadão por ano funcional e bloqueante | Implementado e testado |
| R15 | Dashboard funcional para cidadão e visitante; dashboard do Admin Master depende da definição da expansão global | Implementado para cidadão e visitante |
| R16 | Sistema de notificações in-app e e-mail funcional | Parcial: in-app; e-mail não entregue |
| R17 | Preferências de notificação configuráveis por cidadão | Implementado (in-app) |
| R18 | Interface pública de listagem e resultados funcional; contrato externo separado somente quando definido | Implementado |
| R19 | Server functions autenticadas de criação, assinatura e voto aplicam autorização e invariantes no servidor | Implementado e testado pela rede |
| R20 | Eventos críticos têm integridade verificável conforme modelo de ameaça e mecanismo aprovados | Parcial: cadeia e checkpoints; modelo de ameaça pendente |
| R21 | Auditores independentes verificam integridade sem acessar votos privados | Parcial: verificação local; checkpoints externos dependem do operador |
| R22 | Backup diário auditável e restauração testada, conforme retenção e RPO/RTO aprovados | Parcial: backup/restauração testados localmente; VPS não |
| R23 | Conformidade com WCAG 2.1 AA | Não verificado |
| R24 | Suporte a múltiplos idiomas (i18n) | Parcial: estrutura i18n, só pt-BR |
| R25 | Ambiente local/CI reproduzível compila SSR/WASM e executa os checks definidos, sem resetar dados persistentes | Atendido localmente; CI não executado no GitHub |
| R26 | Frontend instalável como PWA no celular e desktop, sem modo offline ou fila local de ações eleitorais | Implementado; instalação em dispositivo não verificada |
| R27 | Flags persistidas por instância são carregadas na inicialização; alteração no banco mantém o estado efetivo anterior até reinício, quando o novo estado controla servidor e interface; chamadas diretas de funcionalidades desativadas são rejeitadas, sem remover dados ou contornar autorização/regras do domínio | Implementado e testado (E2E com reinício) |
| R28 | Cadastro exige e-mail, senha, nome e endereço; território é derivado do endereço no servidor. Após gravação inicial, tentativas de alterar endereço ou vínculo territorial são rejeitadas e os dados originais permanecem intactos | Implementado e testado |
| R29 | CEP brasileiro consulta BrasilAPI pelo provedor habilitado em feature flag e preenche o endereço. Persistência mantém nomes/estrutura do retorno; falhas, flags conflitantes, dados adulterados e resposta de CEP antigo não produzem cadastro com território incorreto | Implementado e testado com BrasilAPI simulada |

---

## 13. Decisões de produto pendentes

O usuário delegou as escolhas restantes e revisará depois. Todas as bases abaixo são **propostas do agente, pendentes de revisão e implementação**, registradas no [plano SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão). Escolhas expressas pelo usuário continuam preservadas; bloqueadores não são checks aprovados.

| Tema | Base provisória para revisão |
| --- | --- |
| Implantação no Brasil | pt-BR, America/Sao_Paulo, instantes UTC; referência de 4 vCPU/16 GiB e 50 usuários sintéticos. Domínio/VPS/destinos de backup reais são configuração a fornecer, não recursos existentes |
| Identidade e sessão | Argon2id, 12–128 caracteres, revogar todas as sessões após troca/reset; cookie host-only SameSite=Lax + CSRF, 15 min inativo/8h absoluto. Dez códigos TOTP de uso único; sem bypass para perda simultânea de fatores. Métodos de prova de pessoa e vivacidade real são bloqueadores de validação |
| Território e participação | CEP genérico permitido com cidade/UF válidas; catálogo local versionado por IBGE ou correspondência exata cidade/UF. Município/estado/país do endereço inicial, leitura pública entre territórios. Residência/cidadania não comprovadas; demonstração sintética até validação real |
| Eleições | Snapshot imutável por fase; quórum inclui abstenção; ceil para mínimos; SIM>NÃO em normal/urgente e >60% em setup/meta-lei. Empate/zero votos válidos rejeita; eleitorado vazio não abre. Todas são propostas para revisão, não regras de produção aprovadas |
| Setup | Por jurisdição com herança nacional; Admin País só inicia o plebiscito, não adota regras unilateralmente; falha mantém leis bloqueadas. Meta-lei muda apenas eleições futuras, conforme versão publicada |
| Propostas/debate | Quota na publicação; texto imutável; retirada/reassinatura só na coleta; emendas como sugestões; duplicidade gera aviso. Moderação com motivo/histórico, sem reações ou votação de emendas inicialmente |
| Sigilo e auditoria | Sem criptografia própria/blockchain; Helios apenas referência de avaliação, sem adoção para produção. Voto privado e uso eleitoral real bloqueados até protocolo/identidade/operação validados. Eventos minimizados, checkpoints externos; hashes locais não bastam |
| Operação e dados | RPO 1h/RTO 4h, backups cifrados separados, chaves restritas fora do Git/banco; logs 30 dias; dados de conta enquanto ativa, política de remoção antes de produção. Retenção eleitoral não estendida indiscriminadamente a biometria/IP |
| Interface e expansão | pt-BR primeiro, CSS simples, i18n preparada; PWA online com cache só de assets públicos. Sem push, portal/API externa, administração global ou inglês/espanhol na primeira entrega; metas futuras preservadas |
| Feature flags | Painel local do Admin País altera flags conhecidas/auditadas, com aviso de reinício pendente; operador reinicia fora do painel. Ausentes desabilitam features opcionais; auth não pode ser desligada. Integração privada desabilitada até validação |

As bases provisórias permitem preparar a especificação sem novas perguntas nesta entrega. A fundação sintética continua possível; dados/provedores/protocolo real, recuperação operacional e revisão de privacidade continuam bloqueadores de produção descritos no SDD. Não alegar execução, conformidade ou garantia eleitoral por decisão documental. A ordem de trabalho está no [plano SDD](../development/sdd-plan.md); requisitos e tarefas da primeira fatia estão no [guia de fundação](../architecture/foundation-setup.md).

*Democracy Without Candidates — PRD v1.4.0 — 2026-10-25*
