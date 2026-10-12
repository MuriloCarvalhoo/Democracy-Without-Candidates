# Wireframes — Democracia Sem Candidatos

## Finalidade

Atualizado em 2026-10-12. Esboços textuais do produto futuro, alinhados à arquitetura Rust/Leptos do [PRD](../prd/prd.md). Nenhuma tela foi implementada ou validada em navegador. Este documento descreve:

- Fluxos completos de usuário
- Estados de tela (carregamento, erro, vazio)
- Lógica multijurisdicional
- Interações de segurança
- Casos de falha e decisões ainda pendentes

Não resolver regras pela aparência da tela. As decisões abertas estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes); renderização/hidratação e validação estão no [guia da interface](../architecture/frontend-architecture.md).

---

## 1. Fluxo de país e jurisdição

### 1.1 Instância do País

O subdomínio e a configuração identificam o país da implantação, inicialmente Brasil. A mesma aplicação deve ser configurável para outros países. Seleção entre países e catálogo global são fluxos futuros a revisar, sem banco global necessário à instância inicial.

**Estados:**

- Carregando configuração pública da instância
- Instância indisponível
- Configuração de desenvolvimento identificada como teste

---

## 2. Fluxo de autenticação

### 2.1 Login

E-mail
[______]
Credencial conforme contrato de login a decidir
[______]

[ Entrar ]

Estados:

- Credenciais inválidas
- Carregando
- Conta bloqueada
- Sessão expirada ou reautenticação necessária

Senha versus fluxo sem senha ainda é decisão aberta. A tela não define esse contrato.

---

### 2.2 Cadastro

Nome completo
Documento de identidade conforme configuração do país (ex.: CPF no Brasil)
Data de nascimento

[ Continuar ]

Estados:

- Documento inválido
- Restrição de idade
- Conta duplicada

---

### 2.3 Validação Biométrica

[ Escanear Rosto ]
[ Alternativa de acessibilidade/identidade a definir com o provider ]

Estados:

- Sucesso
- Falha (tentar novamente)
- Dispositivo não compatível
- Tempo esgotado (timeout)
- Permissão de câmera negada

Provider real, matching/liveness, recuperação e eventual suporte a impressão digital ainda não foram aprovados. Verificação sintética deve ser rotulada e proibida em produção.

---

### 2.4 Reautenticação (Ações Críticas)

Acionada por:

- Votação
- Assinatura
- Criação de proposta
- Alteração de dados sensíveis/privacidade e participação em mudança de setup

A validação ocorre no servidor e deve estar vinculada à ação/recurso conforme contrato a definir.

---

## 3. Configuração inicial (plebiscito)

Parâmetros:

- Quórum
- Percentual de assinaturas necessárias
- Prazos

Estados:

- Não iniciado
- Votação aberta
- Concluído
- Quórum não atingido

Antes de adotar setup, não criar leis. Quem inicia o plebiscito, suas regras e o caminho após falha de quórum permanecem pendentes; o administrador não altera parâmetros eleitorais diretamente.

---

## 4. Painel principal (dashboard)

Seções:

- Votações ativas
- Propostas em debate
- Coleta de assinaturas

Estados:

- Vazio (sem propostas)
- Carregando
- Erro ao buscar dados

---

## 5. Lista de propostas

Filtros:

- Tipo
- Status
- Nível

Estados:

- Sem resultados
- Carregando
- Filtro sem correspondência

---

## 6. Detalhes da proposta

Componentes:

- Texto completo
- Indicador de status (badge)
- Barra de progresso
- Ações disponíveis (Assinar / Votar)

Estados:

- Arquivada
- Encerrada
- Usuário não elegível

---

## 7. Criar proposta

Validações:

- Campos obrigatórios
- Limite de propostas por usuário/ano
- Detecção de conteúdo duplicado, com política ainda a especificar

Estados:

- Rascunho salvo
- Publicação realizada com sucesso
- Erro de validação
- Setup ainda não adotado

Consumo de quota por rascunho/publicação, edição após apoio e vigência das regras permanecem pendentes.

---

## 8. Coleta de assinaturas

Barra de progresso
Tempo restante

Estados:

- Assinatura removida: cenário proposto; retirada/reassinatura ainda exigem regra aprovada
- Limite atingido
- Prazo expirado

---

## 9. Módulo de debate

Funcionalidades:

- Comentários encadeados (threads)
- Votos positivos/negativos (upvote/downvote)
- Emendas e sugestões de alteração

Estados:

- Comentário removido
- Abuso reportado
- Discussão bloqueada (thread travada)

Moderação, denúncias, reação a comentários e efeito de emendas sobre o texto apoiado/votado precisam de regras explícitas; esses elementos visuais não decidem a política.

---

## 10. Votação

Opções:
[ SIM ] [ NÃO ] [ ABSTENÇÃO ]

Privacidade:
( ) Pública
( ) Privada

Estados:

- Voto já registrado
- Voto registrado com sucesso, somente após aceitação persistida pelo servidor
- Falha ao registrar voto
- Sessão/reautenticação expirada
- Votação encerrada ou eleitor inelegível
- Timeout com resultado ainda desconhecido

O modo privado é o padrão pretendido, com público por consentimento. Protocolo, unicidade entre modos e recibo permanecem pendentes. Em resultado desconhecido, consulta/retry depende do contrato de idempotência; não anunciar sucesso local ou duplicar o efeito.

---

## 11. Resultados

Exibição:

- Quórum
- Percentuais
- Resultado final

Estados:

- Quórum não atingido
- Empate: resultado proposto, com semântica ainda a definir

Exibir versão de regras e referência do eleitorado conforme especificação. Fórmulas, arredondamentos e zero votos válidos permanecem pendentes.

---

## 12. Notificações

Eventos:

- Proposta criada
- Votação aberta
- Lembrete de prazo

Estados:

- Lida / Não lida

---

## 13. Perfil do usuário

Dados:

- Estatísticas de participação
- Configurações de privacidade

Estados:

- Visualização pública
- Visualização privada

---

## 14. Configurações de privacidade

Opções:

- Visibilidade do perfil
- Preferência padrão de voto (público ou privado)

---

## 15. Painel administrativo

Funcionalidades:

- Operação da instância; gerenciamento global de países permanece escopo futuro a revisar
- Estatísticas de usuários
- Saúde do sistema

Estados:

- País inativo
- Configuração pendente

---

## 16. Auditoria e transparência

Dados:

- Logs de atividade
- Evidências de integridade conforme mecanismo a escolher; hash chain é candidato, não garantia validada
- Verificação de integridade

Estados:

- Evidência verificada conforme protocolo/mecanismo definido
- Alerta de integridade
- Dados restritos ao escopo do auditor

Não publicar identidade, biometria ou correlação com cédula privada. Ainda não há verificador implementado.

---

## 17. Visualização de API (modo desenvolvedor)

As server functions Leptos são a interface inicial. Inspeção de requisição/resposta será usada no desenvolvimento; portal e contrato de API externa são escopo futuro a especificar.

---

## 18. Estados globais (aplicáveis a todas as telas)

- Carregando
- Erro
- Vazio
- Sucesso

Sem conexão: indicar indisponibilidade, sem aceitar ou enfileirar assinaturas/votos localmente. O produto não oferece participação em meio físico.

---

## Validação

Estes esboços precisam de implementação e testes de hidratação, mobile, teclado, leitores de tela, erros de rede/sessão e estados críticos. PWA/atualização/cache técnico também exigem especificação; nenhum estado de tela aqui comprova prontidão para produção. O uso é exclusivamente online.
