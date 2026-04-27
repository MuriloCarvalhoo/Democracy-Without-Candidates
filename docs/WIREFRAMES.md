# WIREFRAMES — Democracia Sem Candidatos (NÍVEL ENTERPRISE / PRD)

## 📌 FINALIDADE DO DOCUMENTO
Este documento define wireframes de nível enterprise alinhados 100% ao PRD, incluindo:
- Fluxos completos de usuário
- Estados de tela (carregamento, erro, vazio)
- Lógica multijurisdicional
- Interações de segurança
- Casos extremos (edge cases)

---

# 🌍 1. FLUXO DE PAÍS E JURISDIÇÃO

## 1.1 Seleção de País
[ Brasil ] [ Portugal ] [ Argentina ]

## Estados:
- Carregando países
- Nenhum país disponível
- Erro: seleção obrigatória

---

# 🔐 2. FLUXO DE AUTENTICAÇÃO

## 2.1 Login
E-mail
[______]
Senha
[______]

[ Entrar ]

Estados:
- Credenciais inválidas
- Carregando
- Conta bloqueada

---

## 2.2 Cadastro
Nome completo
CPF / Documento de identidade
Data de nascimento

[ Continuar ]

Estados:
- Documento inválido
- Restrição de idade
- Conta duplicada

---

## 2.3 Validação Biométrica

[ Escanear Rosto ]
[ Impressão Digital ]

Estados:
- Sucesso
- Falha (tentar novamente)
- Dispositivo não compatível
- Tempo esgotado (timeout)

---

## 2.4 Reautenticação (Ações Críticas)

Acionada por:
- Votação
- Assinatura
- Criação de proposta

---

# ⚙️ 3. CONFIGURAÇÃO INICIAL (PLEBISCITO)

Parâmetros:
- Quórum
- Percentual de assinaturas necessárias
- Prazos

Estados:
- Não iniciado
- Votação aberta
- Concluído
- Quórum não atingido

---

# 🏠 4. PAINEL PRINCIPAL (DASHBOARD)

Seções:
- Votações ativas
- Propostas em debate
- Coleta de assinaturas

Estados:
- Vazio (sem propostas)
- Carregando
- Erro ao buscar dados

---

# 📋 5. LISTA DE PROPOSTAS

Filtros:
- Tipo
- Status
- Nível

Estados:
- Sem resultados
- Carregando
- Filtro sem correspondência

---

# 📄 6. DETALHES DA PROPOSTA

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

# ✍️ 7. CRIAR PROPOSTA

Validações:
- Campos obrigatórios
- Limite de propostas por usuário/ano
- Detecção de conteúdo duplicado

Estados:
- Rascunho salvo
- Publicação realizada com sucesso
- Erro de validação

---

# ✍️ 8. COLETA DE ASSINATURAS

Barra de progresso
Tempo restante

Estados:
- Assinatura removida
- Limite atingido
- Prazo expirado

---

# 💬 9. MÓDULO DE DEBATE

Funcionalidades:
- Comentários encadeados (threads)
- Votos positivos/negativos (upvote/downvote)
- Emendas e sugestões de alteração

Estados:
- Comentário removido
- Abuso reportado
- Discussão bloqueada (thread travada)

---

# 🗳️ 10. VOTAÇÃO

Opções:
[ SIM ] [ NÃO ] [ ABSTENÇÃO ]

Privacidade:
( ) Pública
( ) Privada

Estados:
- Voto já registrado
- Voto registrado com sucesso
- Falha ao registrar voto

---

# 📊 11. RESULTADOS

Exibição:
- Quórum
- Percentuais
- Resultado final

Estados:
- Quórum não atingido
- Empate

---

# 🔔 12. NOTIFICAÇÕES

Eventos:
- Proposta criada
- Votação aberta
- Lembrete de prazo

Estados:
- Lida / Não lida

---

# 👤 13. PERFIL DO USUÁRIO

Dados:
- Estatísticas de participação
- Configurações de privacidade

Estados:
- Visualização pública
- Visualização privada

---

# 🔐 14. CONFIGURAÇÕES DE PRIVACIDADE

Opções:
- Visibilidade do perfil
- Preferência padrão de voto (público ou privado)

---

# ⚙️ 15. PAINEL ADMINISTRATIVO

Funcionalidades:
- Gerenciamento de países
- Estatísticas de usuários
- Saúde do sistema

Estados:
- País inativo
- Configuração pendente

---

# 📊 16. AUDITORIA E TRANSPARÊNCIA

Dados:
- Logs de atividade
- Cadeia de hash (hash chain)
- Verificação de integridade

Estados:
- Verificado
- Alerta de integridade

---

# 🔗 17. VISUALIZAÇÃO DE API (MODO DESENVOLVEDOR)

- Lista de endpoints
- Pré-visualização de requisição e resposta

---

# 🧭 18. ESTADOS GLOBAIS (APLICÁVEIS A TODAS AS TELAS)

- Carregando
- Erro
- Vazio
- Sucesso

---

# 🚀 CONCLUSÃO

Este documento representa uma estrutura de wireframes pronta para produção, alinhada a sistemas de nível enterprise e ao escopo completo do PRD.
