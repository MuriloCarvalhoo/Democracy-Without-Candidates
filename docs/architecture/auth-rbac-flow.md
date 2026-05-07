# AUTH-RBAC-FLOW.md

# Democracy Without Candidates
## Authentication & RBAC Flow Guide

> Versão: 1.0.0  
> Status: Foundation Phase  
> Stack: NestJS + JWT + Face Auth + TOTP + RBAC

---

# 1. Objetivo

Este documento define o fluxo oficial de autenticação, autorização e controle de permissões do projeto **Democracy Without Candidates**.

O objetivo é garantir que o sistema possua:

- Login seguro
- Autenticação biométrica
- Segundo fator de autenticação
- Reautenticação para ações críticas
- Controle de acesso por papéis
- Controle de permissões por recurso
- Separação clara entre usuários, roles e permissões
- Base segura para propostas, assinaturas e votos

---

# 2. Conceitos Principais

| Conceito | Descrição |
|---|---|
| Auth | Processo de autenticar a identidade do usuário |
| RBAC | Controle de acesso baseado em papéis |
| Role | Papel do usuário dentro do sistema |
| Permission | Permissão específica para executar uma ação |
| JWT | Token de autenticação usado nas requisições |
| 2FA | Segundo fator de autenticação |
| TOTP | Código temporário gerado por apps como Google Authenticator |
| Biometria | Validação facial usada para confirmar identidade |
| Reautenticação | Nova validação exigida para ações críticas |

---

# 3. Estratégia Geral

O sistema utilizará autenticação em múltiplas camadas:

```txt
E-mail/Senha + Biometria Facial + 2FA + JWT + RBAC
```

Fluxo base:

```txt
1. Usuário informa credenciais
2. Sistema valida e-mail/senha
3. Sistema valida biometria facial
4. Sistema solicita código 2FA
5. Sistema emite JWT
6. Sistema aplica RBAC nas rotas protegidas
```

---

# 4. Roles Oficiais

| Role | Descrição |
|---|---|
| VISITOR | Usuário não autenticado |
| PENDING_CITIZEN | Cidadão cadastrado com validação pendente |
| CITIZEN | Cidadão validado e autenticado |
| AUDITOR | Auditor independente |
| COUNTRY_ADMIN | Administrador de país ou jurisdição |
| MASTER_ADMIN | Administrador global da plataforma |

---

# 5. Permissões Oficiais

As permissões devem seguir o padrão:

```txt
resource:action
```

Exemplos:

```txt
proposal:create
proposal:update
proposal:sign
proposal:vote
proposal:comment
setup:vote
setup:update
audit:read
audit:advanced_read
country:create
country:update
user:manage
role:manage
```

---

# 6. Matriz Inicial de Permissões

| Role | Permissões principais |
|---|---|
| VISITOR | proposal:read, result:read, statistics:read |
| PENDING_CITIZEN | profile:complete, biometric:validate |
| CITIZEN | proposal:create, proposal:sign, proposal:vote, proposal:comment, profile:update |
| AUDITOR | audit:read, audit:advanced_read |
| COUNTRY_ADMIN | country:read, jurisdiction:manage, setup:manage |
| MASTER_ADMIN | country:create, country:update, auditor:manage, role:manage, system:manage |

---

# 7. Cadastro de Cidadão

Fluxo de cadastro:

```txt
1. Usuário informa nome, e-mail, documento e data de nascimento
2. Sistema valida idade mínima e documento
3. Sistema verifica duplicidade por jurisdição
4. Sistema cria usuário com status PENDING_CITIZEN
5. Sistema direciona para validação biométrica
6. Usuário realiza captura facial
7. Sistema gera hash biométrico
8. Sistema ativa 2FA
9. Usuário passa para status CITIZEN
```

---

# 8. Login

Fluxo de login:

```txt
1. Usuário informa e-mail e senha
2. Sistema valida credenciais
3. Sistema solicita validação facial
4. Sistema valida hash biométrico
5. Sistema solicita código TOTP
6. Sistema valida 2FA
7. Sistema emite access token e refresh token
8. Usuário acessa o sistema conforme suas permissões
```

---

# 9. JWT

O JWT deve conter apenas dados necessários.

Exemplo de payload:

```json
{
  "sub": "user_uuid",
  "email": "user@email.com",
  "role": "CITIZEN",
  "permissions": ["proposal:create", "proposal:vote"],
  "jurisdictionId": "jurisdiction_uuid",
  "countryId": "country_uuid"
}
```

## Regras

- Nunca incluir dados sensíveis no JWT
- Nunca incluir senha
- Nunca incluir documento completo
- Nunca incluir segredo 2FA
- Nunca incluir biometria
- Expiração curta para access token
- Refresh token com controle seguro no backend

---

# 10. Refresh Token

O refresh token será usado para renovar sessão sem novo login completo.

Regras:

- Deve ser armazenado de forma segura
- Deve ser revogável
- Deve possuir expiração
- Deve ser invalidado no logout
- Deve ser invalidado em atividade suspeita

---

# 11. Biometria Facial

A biometria será usada para garantir:

```txt
1 pessoa = 1 conta
```

O projeto poderá usar como base inicial o repositório auth-face.

## Regras obrigatórias

- Não armazenar imagem facial bruta
- Armazenar apenas hash biométrico
- Usar provider/service abstraído
- Permitir troca futura do provider
- Registrar tentativas de validação
- Bloquear excesso de tentativas falhas

---

# 12. Provider de Biometria

A implementação deve ser desacoplada.

Exemplo conceitual:

```txt
BiometricProvider
├── AuthFaceProvider
├── FutureGovProvider
└── MockProvider
```

Benefícios:

- Troca futura sem quebrar o sistema
- Testes facilitados
- Menor acoplamento
- Evolução técnica gradual

---

# 13. 2FA / TOTP

O segundo fator será baseado em TOTP.

Exemplos de apps compatíveis:

```txt
Google Authenticator
Microsoft Authenticator
Authy
1Password
```

## Regras

- Obrigatório para login completo
- Obrigatório para ações críticas
- Secret deve ser criptografado
- Backup codes podem ser adicionados futuramente
- Desativação do 2FA deve exigir reautenticação

---

# 14. Reautenticação para Ações Críticas

Algumas ações exigem revalidação completa:

```txt
Biometria + 2FA
```

Ações críticas:

| Ação | Reautenticação |
|---|---|
| Criar proposta | Obrigatória |
| Assinar proposta | Obrigatória |
| Votar | Obrigatória |
| Alterar privacidade | Obrigatória |
| Alterar dados sensíveis | Obrigatória |
| Alterar setup | Obrigatória |

---

# 15. Fluxo de Voto com Reautenticação

```txt
1. Usuário acessa proposta em votação
2. Sistema verifica se usuário é CITIZEN
3. Sistema verifica se usuário pertence à jurisdição
4. Sistema verifica se ainda não votou
5. Sistema solicita biometria
6. Sistema solicita código 2FA
7. Usuário escolhe SIM, NÃO ou ABSTENÇÃO
8. Usuário escolhe voto público ou privado
9. Sistema registra voto
10. Sistema gera log de auditoria
11. Sistema gera comprovante de voto
```

---

# 16. Fluxo de Assinatura com Reautenticação

```txt
1. Usuário acessa proposta em coleta
2. Sistema verifica elegibilidade
3. Sistema verifica se ainda não assinou
4. Sistema solicita biometria
5. Sistema solicita código 2FA
6. Sistema registra assinatura
7. Sistema atualiza contador
8. Sistema gera log de auditoria
```

---

# 17. Fluxo de Criação de Proposta

```txt
1. Usuário autenticado acessa criação
2. Sistema verifica role CITIZEN
3. Sistema verifica limite anual
4. Usuário preenche proposta
5. Sistema valida dados
6. Sistema solicita reautenticação
7. Sistema cria proposta em rascunho ou coleta
8. Sistema gera log de auditoria
```

---

# 18. Guards no Backend

Guards recomendados:

```txt
JwtAuthGuard
RolesGuard
PermissionsGuard
BiometricReauthGuard
JurisdictionGuard
ProposalOwnerGuard
SetupGuard
```

---

# 19. Decorators no Backend

Decorators recomendados:

```ts
@Public()
@CurrentUser()
@Roles('CITIZEN')
@Permissions('proposal:create')
@RequireBiometricReauth()
@JurisdictionScoped()
```

---

# 20. Controle de Acesso no Frontend

O frontend deve controlar acesso visualmente, mas a segurança real sempre fica no backend.

Exemplos:

- Esconder botão de votar para usuário não autenticado
- Esconder painel admin para cidadão comum
- Redirecionar visitante para login
- Exibir mensagem de permissão insuficiente

Importante:

```txt
O frontend não é fonte de segurança.
O backend sempre deve validar permissões novamente.
```

---

# 21. Rotas Protegidas no Frontend

Exemplos:

```txt
PrivateRoute
RoleRoute
PermissionRoute
AdminRoute
CitizenRoute
```

---

# 22. Estados de Autenticação no Frontend

Estados esperados:

```txt
unauthenticated
pending_citizen
authenticated
reauth_required
session_expired
forbidden
```

---

# 23. Sessão Expirada

Quando a sessão expirar:

```txt
1. Sistema tenta renovar com refresh token
2. Se falhar, redireciona para login
3. Se ação crítica, exige reautenticação completa
```

---

# 24. Segurança Contra Abusos

Medidas recomendadas:

- Rate limit
- Bloqueio por excesso de tentativas
- Logs de login
- Logs de falha biométrica
- Detecção de comportamento suspeito
- Revogação de sessão
- Timeout em ações críticas

---

# 25. Auditoria de Autenticação

Eventos auditáveis:

```txt
user_registered
user_login_success
user_login_failed
biometric_validation_success
biometric_validation_failed
two_factor_success
two_factor_failed
token_refreshed
logout
critical_reauth_success
critical_reauth_failed
permission_denied
```

---

# 26. Estratégia de Erros

Erros devem ser claros, porém sem vazar informação sensível.

Exemplos:

```txt
Credenciais inválidas
Acesso negado
Reautenticação necessária
Permissão insuficiente
Sessão expirada
Conta bloqueada
```

Evitar mensagens como:

```txt
E-mail existe no banco
Documento já pertence a outro usuário
Hash biométrico encontrado
```

Essas mensagens podem expor dados sensíveis.

---

# 27. Separação de Responsabilidades

## Auth Module

Responsável por:

- Login
- Registro
- JWT
- Refresh token
- 2FA
- Biometria
- Reautenticação

---

## RBAC Module

Responsável por:

- Roles
- Permissões
- Guards
- Decorators
- Políticas

---

## Users Module

Responsável por:

- Perfil
- Dados pessoais
- Status do cidadão
- Preferências de privacidade

---

# 28. Ordem Recomendada de Implementação

## Fase 1 — Auth Base

- Users
- Password hash
- JWT
- Refresh token
- Login básico

---

## Fase 2 — RBAC

- Roles
- Permissions
- Guards
- Decorators
- Seeds iniciais

---

## Fase 3 — 2FA

- TOTP secret
- QR Code
- Validação de código
- Recuperação futura

---

## Fase 4 — Biometria

- Provider abstrato
- Integração auth-face
- Validação facial
- Logs de tentativa

---

## Fase 5 — Reautenticação Crítica

- Voto
- Assinatura
- Criação de proposta
- Alteração de perfil sensível

---

# 29. O que NÃO fazer agora

Evitar nesta fase:

- Acoplar auth-face diretamente nos controllers
- Armazenar imagem facial bruta
- Colocar dados sensíveis no JWT
- Confiar apenas no frontend para permissões
- Misturar Auth com RBAC sem separação
- Criar permissões hardcoded espalhadas
- Criar regra de negócio dentro de guards
- Implementar biometria final antes do MVP estrutural

---

# 30. Critérios de Aceite

| ID | Critério |
|---|---|
| AUTH01 | Usuário consegue se cadastrar |
| AUTH02 | Usuário inicia como PENDING_CITIZEN |
| AUTH03 | Usuário validado vira CITIZEN |
| AUTH04 | Login emite JWT válido |
| AUTH05 | JWT não contém dados sensíveis |
| AUTH06 | RBAC bloqueia rotas protegidas |
| AUTH07 | Guards validam roles e permissões |
| AUTH08 | Ações críticas exigem reautenticação |
| AUTH09 | Voto exige biometria + 2FA |
| AUTH10 | Assinatura exige biometria + 2FA |
| AUTH11 | Criação de proposta exige reautenticação |
| AUTH12 | Logs de autenticação são auditados |

---

# 31. Conclusão

O fluxo de autenticação e RBAC do Democracy Without Candidates deve priorizar:

- Segurança
- Clareza
- Separação de responsabilidades
- Controle de permissões
- Reautenticação em ações críticas
- Baixo acoplamento com providers externos
- Evolução gradual da biometria
- Proteção de dados sensíveis

A autenticação e o RBAC são partes centrais do sistema, pois sustentam a confiança, a integridade dos votos e a legitimidade da participação cidadã.

---

# Democracy Without Candidates
## Authentication & RBAC Flow Guide
