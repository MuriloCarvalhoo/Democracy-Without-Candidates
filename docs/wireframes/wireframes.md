# Wireframes — Democracia Sem Candidatos

## Finalidade

Atualizado em 2026-10-12. Esboços textuais do produto futuro, alinhados à arquitetura Rust/Leptos do [PRD](../prd/prd.md). Nenhuma tela foi implementada ou validada em navegador. Este documento descreve:

- Fluxos completos de usuário
- Estados de tela (carregamento, erro, vazio)
- Lógica multijurisdicional
- Interações de segurança
- Casos de falha e decisões ainda pendentes

Não resolver regras pela aparência da tela. As decisões restantes foram delegadas e estão como propostas para revisão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes); renderização/hidratação e validação estão no [guia da interface](../architecture/frontend-architecture.md).

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
Senha
[______]

[ Entrar ]

Sem opção “manter conectado” e sem persistência de login entre usos. Sessão expira após 15 minutos sem atividade; solicitar novo login, sem reenviar automaticamente ações pendentes.

Estados:

- Credenciais inválidas
- Carregando
- Conta bloqueada
- Sessão expirada ou reautenticação necessária

Após validar e-mail/senha, exigir verificação facial e código TOTP antes de estabelecer sessão. Informar falha sem revelar contas de terceiros. Perda do TOTP: oferecer entrada de código de recuperação de uso único, mantendo e-mail/senha e rosto. Senha esquecida: informar e-mail, validar rosto + TOTP e definir nova senha, sem envio de e-mail. Falhas não alteram a senha; endereço continua bloqueado.

---

### 2.2 Cadastro

Nome (obrigatório)
E-mail (obrigatório)
Senha (obrigatória)
CEP (obrigatório no Brasil)
[ Buscar endereço ]
Estado, cidade, bairro e logradouro preenchidos pelo BrasilAPI
País identificado pela instância

[ Continuar ]

Confirmar o endereço antes de concluir: após cadastro, ele fica bloqueado para alteração nesta versão. Não pedir número ou complemento nesta versão. Não exigir confirmação de e-mail para ativação; biometria e TOTP continuam no fluxo. Fornecer códigos de recuperação TOTP de uso único no cadastro, orientando guardar em local seguro. CEPs genéricos e demais requisitos de elegibilidade ainda serão especificados.

Estados:

- Campo obrigatório ausente ou inválido
- Buscando CEP
- CEP inválido ou não encontrado
- Provedor desativado, indisponível ou timeout
- Endereço/território inválido ou não resolvido
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

Antes de adotar setup, não criar leis. As [propostas do SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão) definem início pelo Admin País, pacote sugerido e leis bloqueadas após falha/rejeição; o administrador não altera parâmetros eleitorais diretamente.

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
- Aviso de conteúdo duplicado, sem bloqueio automático — proposta para revisão

Estados:

- Rascunho salvo
- Publicação realizada com sucesso
- Erro de validação
- Setup ainda não adotado

Propostas para revisão: quota na publicação, texto imutável após publicação e regras novas apenas para futuras eleições, conforme o SDD.

---

## 8. Coleta de assinaturas

Barra de progresso
Tempo restante

Estados:

- Assinatura retirada/reassinada somente na coleta — proposta para revisão
- Limite atingido
- Prazo expirado

---

## 9. Módulo de debate

Funcionalidades:

- Comentários encadeados (threads)
- Reações a comentários fora da versão inicial — proposta para revisão
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
- Empate: Rejeitada com motivo empate — proposta para revisão

Exibir versão de regras e referência do eleitorado conforme especificação. Fórmulas/arredondamentos e rejeição com zero votos válidos seguem propostas do SDD, ainda sem execução.

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

Endereço cadastrado é privado e aparece somente para leitura ao titular; não oferecer ação de alteração nesta versão.

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
