# Identidade, sessão e autorização

Atualizado em 2026-10-25. Implementado em Rust/Leptos/Axum com provider facial sintético e cliente do motor Python; biometria real não validada. Requisitos no [PRD](../prd/prd.md), decisões e bloqueadores no [plano SDD](../development/sdd-plan.md).

## Limites do desenho

E-mail é o identificador obrigatório de login. Cadastro exige e-mail, senha, nome e endereço, sem CPF ou outro documento nacional. Login exige senha, verificação facial e TOTP; informar um e-mail não comprova controle da conta nem unicidade de pessoa.

O produto pretende comprovar identidade e impedir contas/votos duplicados, com biometria facial e segundo fator TOTP. As decisões de identidade e sessão estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes). Biometria e 2FA não são, por si, evidência de unicidade eleitoral.

Provider de teste permite cenários determinísticos, nunca comprova identidade real e deve ser explicitamente habilitado e rejeitado em produção.

## Autorização no modelo IAM

Pedido do usuário (2026-10-25): permissionamento igual ao da AWS, conta admin master com tudo liberado na seed e cadastro novo sem ver tudo. Decisão do usuário: toda conta que fica ativa recebe automaticamente a política gerenciada `Cidadao`; administração, moderação, auditoria e flags só por políticas adicionais anexadas por um administrador. As demais escolhas abaixo são propostas do agente para revisão.

- **Documento de política** (JSON validado no servidor, até 8 KiB e 32 declarações): `{"Version":"2026-10-25","Statement":[{"Sid":"...","Effect":"Allow"|"Deny","Action":[...],"Resource":[...]}]}`. `Action` aceita apenas o catálogo fixo em código (`domain::iam::Permission`), `*`, `serviço:*` e prefixo terminado em `*`; ação desconhecida, `Condition`, `NotAction` e outras chaves são recusadas. `Resource` aceita `*`, `jurisdiction:*` e `jurisdiction:<código IBGE>` (com `*` no fim, ex.: `jurisdiction:42*`).
- **Avaliação** por requisição: Deny explícito vence qualquer Allow; sem correspondência é negação implícita. Checagens sem recurso usam o recurso `*`, que só casa com o padrão `*` (declaração restrita a jurisdição nunca concede ação global). Checagens territoriais (`moderation:act` em comentário/trava de debate) usam `jurisdiction:<código>`; o código só é consultado quando alguma declaração anexada é restrita.
- **Catálogo:** `profile:read`, `profile:update`, `proposal:read`, `proposal:create`, `proposal:sign`, `proposal:vote`, `proposal:archive`, `comment:create`, `setup:start`, `setup:vote`, `moderation:act`, `flags:manage`, `admin:read`, `audit:read`, `audit:read_restricted`, `stats:read`, `iam:read`, `iam:manage`.
- **Políticas gerenciadas** (migration `0500_iam`, somente leitura também por trigger): `AdministratorAccess` (`*` em `*`), `Cidadao` (perfil, leitura, criar/assinar/votar propostas, comentar, votar setup, auditoria pública, estatísticas), `Moderador` (`moderation:act`), `Auditor` (`audit:read`, `audit:read_restricted`) e `Publico` (`proposal:read`, `audit:read`, `stats:read`).
- **Principais:** usuários e grupos; uma política pode ser anexada diretamente ou via grupo. Visitante é avaliado só contra `Publico`; toda conta pendente/ativa recebe `Publico` e a base em código `Conta` (`profile:read`).
- **Situação da conta domina:** bloqueada → nada; pendente → `Conta` + `Publico` (anexos ignorados); ativa → `Conta` + `Publico` + anexos. Política nunca substitui elegibilidade (território do endereço imutável, eleitorado materializado), autoria, fase ou reautenticação.
- **Eleitorado:** contas ativas com `Cidadao` anexada (direta ou por grupo). Um Deny de `proposal:vote` impede votar, mas a conta continua contada no eleitorado.
- **Guardas:** só quem tem `iam:manage` altera políticas/grupos/anexos; cada mudança consome uma reautenticação (rosto + TOTP) `iam:manage`/`iam` na mesma transação, gera evento de auditoria restrito `iam.*` (sem segredos) e é recusada se deixar a instância sem titular ativo de `AdministratorAccess` capaz de `iam:manage` (inclui Deny anexado a si mesmo, direto ou por grupo). Política de cliente só é excluída depois de desanexada; gerenciadas não são editáveis nem excluíveis.
- **Sem cache:** as políticas do ator são carregadas a cada requisição (consulta à tabela de usuário + uma consulta de políticas), então anexar/desanexar vale na requisição seguinte.
- **Bootstrap:** `dwc grant-admin <email>` (serviço parado) anexa `AdministratorAccess` a uma conta ativa, auditado como `operator:cli`. A seed de desenvolvimento cria `master@dev.invalid` ("Admin Master Sintético", `AdministratorAccess` + `Cidadao`).
- **Interface:** `/admin/iam` (políticas com editor JSON, grupos, usuários por e-mail com permissões efetivas, simulador usuário + ação + recurso → Allow/Deny e declaração decisiva). Menus seguem as ações permitidas do visitante (`Viewer.allowed`); o servidor continua sendo a fonte de verdade.

## Cadastro e status

1. Receber e-mail, senha, nome e endereço obrigatórios, sem documento nacional; validar formato, consentimentos aplicáveis e critérios de elegibilidade ainda a especificar.
2. Criar identidade pendente, tratando duplicidade sem revelar contas de terceiros.
3. Executar prova de identidade pelo provider, com falhas, timeout e bloqueio de tentativas definidos.
4. Configurar segundo fator e política de recuperação conforme o fluxo aprovado.
5. Ativar a conta apenas após as verificações necessárias, anexando `Cidadao`; fixtures sintéticas permanecem rotuladas como teste.

O endereço gravado no primeiro cadastro define cidade, estado e país de participação; alterações posteriores estão bloqueadas nesta versão, inclusive por chamadas diretas. Derivar o vínculo territorial no servidor a partir do endereço, sem confiar em IDs de jurisdição livres enviados pelo cliente. Endereço informado não comprova residência por si só; a busca por CEP usa o [provedor de endereço](backend-architecture.md#provedor-de-endereço) selecionado por flag. Tratamento de CEPs genéricos, resolução territorial e demais critérios de elegibilidade estão no PRD. Não persistir imagem facial bruta; forma de referência/template protegido depende do provedor. Não usar igualdade de um hash como especificação de matching biométrico.

## Login, sessão e recuperação

Login aprovado: e-mail identifica a conta; senha, verificação facial e TOTP autenticam. Estabelecer sessão somente após validar os fatores. Nesta versão, não exigir link de confirmação de e-mail para ativar a conta. Validar formato do e-mail, mantendo as demais verificações de cadastro, rosto e TOTP; não apresentar o endereço de e-mail informado como verificado.

Não oferecer opção “manter conectado” nem credenciais de login em armazenamento persistente do navegador. A sessão não deve persistir entre usos. Verificar fechamento/reabertura e restauração de abas em navegador real antes de afirmar esse comportamento; o servidor não deve depender apenas de evento de fechamento para expirar/revogar sessão. A sessão expira após 15 minutos sem atividade, com decisão aplicada no servidor. As [bases provisórias do SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão) propõem limite absoluto de 8 horas e renovação por navegação/ações explícitas autenticadas, sem polling. Estas escolhas aguardam revisão; os 15 minutos de inatividade já foram definidos pelo usuário.

Persistir somente hash de senha com salt, sem texto puro, logs ou devolução da senha ao cliente. Recuperar senha esquecida com verificação facial e TOTP da conta identificada pelo e-mail, sem envio de link/e-mail. Somente após validar os fatores permitir definir nova senha; falha mantém o hash anterior e não autoriza editar endereço/território. As [bases provisórias do SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão) propõem Argon2id, senha de 12–128 caracteres e revogação de todas as sessões após troca/reset. Perda simultânea de senha, TOTP e códigos não tem bypass nesta versão; manter acesso bloqueado.

Definir o contrato de sessão antes de implementar: renovação, revogação, logout, proteção de requisições e recuperação após perda de dispositivo/TOTP. Persistir sessões no TursoDB da instância, com identificador opaco em cookie de sessão, sem JWT de login. Preservar estado de revogação e última atividade após reinício; o reinício não reinicia o prazo de 15 minutos nem reativa sessões expiradas/revogadas. Persistência no servidor não habilita “manter conectado” no navegador. Cookie HttpOnly, Secure em produção HTTPS e limitado ao host da instância; não compartilhar cookies entre países. Não usar cookie persistente de “manter conectado”. Schema/migrations serão verificados no motor real. Política provisória: cookie __Host-dwc_session, SameSite=Lax e proteção CSRF nas chamadas mutáveis, conforme o SDD; detalhe de biblioteca depende do scaffold. Não incluir material biométrico, segredo TOTP ou dados eleitorais privados em tokens ou payloads públicos.

Segredos do segundo fator exigem proteção e gestão de chaves especificadas. Para perda do autenticador TOTP, fornecer códigos de recuperação de uso único durante o cadastro. Um código válido substitui apenas TOTP nesse fluxo; senha e rosto continuam obrigatórios. Guardar somente hashes, consumir o código atomicamente e rejeitar código inválido, já usado ou duas tentativas concorrentes com o mesmo código. As [bases provisórias do SDD](../development/sdd-plan.md#decisões-delegadas-para-revisão) propõem dez códigos de 128 bits, regeneração com senha+rosto+TOTP revogando os anteriores, reenrolamento após uso e limitação de tentativas. Validar replay/concorrência e recuperação real antes de produção.

No limite de 15 minutos sem atividade, chamadas protegidas devem ser rejeitadas e exigir novo login; não confiar apenas no timer do navegador nem reaplicar ações críticas silenciosamente. Testar antes, no limite e após a expiração com tempo controlado, incluindo chamadas diretas.

Estados da interface: visitante, cadastro pendente, autenticado, reautenticação necessária, sessão expirada, acesso negado e conta bloqueada. Renovação de sessão não equivale a reautenticação de uma ação crítica.

## Aproveitamento do auth-face

Avaliação estática em 2026-10-12 do [auth-face, commit d873629](https://github.com/MuriloCarvalhoo/auth-face/tree/d873629f6f84fd858162e3a73435002811d903cc). Nenhum serviço, teste, câmera ou integração foi executado. O usuário aprovou reaproveitar apenas o motor facial Python na mesma VPS; sua integração ainda não foi implementada ou validada.

| Área | Resultado da inspeção e ajuste necessário |
| --- | --- |
| E-mail/login | E-mail opcional no cadastro; login facial compara com todas as contas, sem e-mail ou senha. Rust deverá implementar o fluxo e-mail + senha aprovado, utilizando só o motor facial Python |
| Link por e-mail | Token é gerado e mostrado no console, sem envio real; consumo usa leitura e atualização separadas. Não adotar como login alternativo que contorne senha/rosto/TOTP. Não adotar o link para recuperação de senha: o fluxo aprovado usa rosto + TOTP, sem envio de e-mail |
| Biometria | DeepFace/Facenet512 extrai e compara vetores; piscada é verificada só no navegador. Backend recebe uma imagem, sem comprovação de liveness. Imagem bruta é gravada temporariamente no disco; proteção dos vetores e tratamento de falsos resultados exigem validação |
| Sessão e proteção | JWT com segredo de fallback e prazo de sete dias; armazenamento no navegador via Zustand persist. Sem TOTP ou limitação de tentativas aplicada nas rotas inspecionadas. Não adotar JWT ou persistência de login do protótipo; a sessão da aplicação é no servidor com cookie. Corrigir antes de uso real; cadastro concorrente também precisa de controle de duplicidade |
| Integração | Aproveitar apenas o motor Python/Flask como serviço privado na mesma VPS. Leptos/Rust mantém UI, contas, sessões e TursoDB; não incorporar React/Express/PostgreSQL. Adaptar o serviço para loopback, acesso interno controlado e tratamento validado de imagens |

Fontes: [rotas de autenticação](https://github.com/MuriloCarvalhoo/auth-face/blob/d873629f6f84fd858162e3a73435002811d903cc/artifacts/api-server/src/routes/auth.ts), [serviço facial](https://github.com/MuriloCarvalhoo/auth-face/blob/d873629f6f84fd858162e3a73435002811d903cc/artifacts/face-service/main.py) e [captura no navegador](https://github.com/MuriloCarvalhoo/auth-face/blob/d873629f6f84fd858162e3a73435002811d903cc/artifacts/facial-auth/src/components/FaceScanner.tsx).

## Reautenticação e autorização no servidor

Ações críticas previstas: criar proposta, assinar, votar, alterar dados sensíveis/privacidade e participar de alterações do setup. A validade, o vínculo a recurso/ação e o consumo/replay da prova de reautenticação devem ser especificados; o navegador não pode declarar que a verificação ocorreu.

| Camada | Verificação |
| --- | --- |
| Endpoint Leptos/Axum | Entrada limitada, sessão obtida no servidor e proteção de transporte |
| Aplicação | Status, permissão, autoria, jurisdição, fase, setup e reautenticação |
| Domínio/persistência | Invariantes, duplicidade, quotas e efeito transacional |
| Interface | Apresentação das condições e tratamento de erros; sem autoridade final |

Para voto, revalidar elegibilidade e fase junto à aceitação, inclusive disputa com encerramento. Para assinatura, exigir fase de coleta e unicidade. Para criação/publicação, exigir autoria, limite aplicável e setup adotado. A proposta do SDD consome quota atomicamente na publicação, sem consumir em rascunho ou devolver após cancelamento.

Não vincular automaticamente a cédula privada a registro de reautenticação identificável. O protocolo precisa coordenar elegibilidade, prova de ação e unicidade sem contradizer a promessa de sigilo.

## Auditoria e erros

Eventos relevantes: cadastro, login/falha, verificação de identidade, segundo fator, renovação/revogação de sessão, reautenticação e acesso negado. Definir dados mínimos, separação de logs operacionais e evidências públicas e retenção por categoria.

Mensagens devem permitir corrigir credenciais, sessão expirada e reautenticação sem revelar que determinado e-mail/biometria pertence a outra pessoa. Registrar falha de provider ou banco sem transformar erro em cidadão validado. Não expor biometria, segredos, dados pessoais ou associação com voto privado.

## Critérios de aceitação pendentes

Os IDs AUTH01–AUTH14 são estáveis. A coluna PRD relaciona critérios aos [requisitos do produto](../prd/prd.md#12-critérios-de-aceite); um critério isolado não encerra esses requisitos. Testes com identidade sintética não comprovam biometria real nem unicidade de pessoa.

| ID | Critério e evidência necessária | PRD relacionado | Estado em 2026-10-25 |
| --- | --- | --- | --- |
| AUTH01 | Cadastro exige e-mail, senha, nome e endereço; campos obrigatórios ausentes/inválidos ou falhas do provider não ativam a conta | R02, R28, R29 | Implementado e testado (provider sintético) |
| AUTH02 | Cadastro inicia em PENDING_CITIZEN e não permite ações eleitorais | R02, R03 | Implementado e testado |
| AUTH03 | Ativação não exige confirmação de e-mail, mas exige as demais verificações aprovadas; falhas não ativam CITIZEN e teste sintético não valida provider real | R02, R03 | Sintético testado; provider real não validado |
| AUTH04 | Login usa e-mail + senha, rosto e TOTP, ou código válido no fluxo de recuperação de TOTP; falha em senha/rosto ou segundo fator sem recuperação válida não estabelece sessão | R02, R19 | Implementado; E2E de login completo |
| AUTH05 | Sessão persiste no TursoDB e mantém expiração/revogação após reinício, com cookie opaco HttpOnly/Secure em produção e isolado por host; payloads públicos não contêm dados sensíveis; não há manter conectado ou persistência de login entre usos; expiração por 15 minutos de inatividade, revogação e reabertura/restauração do navegador são testadas | R19, RNF10 | Persistência, expiração e revogação testadas; fechamento/restauração do navegador não testados |
| AUTH06 | RBAC nega chamadas de rede protegidas sem autorização | R03, R19 | Testado pela rede |
| AUTH07 | Permissão considera o território derivado do endereço cadastrado, autoria e condições do recurso; endereço não pode ser alterado após cadastro | R03, R13, R19, R28 | Testado, incluindo rejeição de alteração de endereço |
| AUTH08 | Ação crítica rejeita reautenticação ausente, inválida, expirada ou para outra ação | R09, R11, R19 | Testado |
| AUTH09 | Voto exige identidade/elegibilidade e face + 2FA conforme provider/protocolo aprovados | R02, R05, R11 | Parcial: voto público demonstrativo com identidade sintética |
| AUTH10 | Assinatura exige reautenticação prevista e não aceita duplicidade | R09, R19 | Testado, incluindo concorrência no limiar |
| AUTH11 | Criar proposta exige reautenticação, setup e limites aplicáveis | R06, R14, R19 | Testado |
| AUTH12 | Eventos relevantes são auditáveis sem expor segredos ou comprometer voto privado | R20, R21, RNF10 | Eventos implementados sem segredos; verificação externa pendente |
| AUTH13 | Recuperação de TOTP usa código de uso único, mantendo senha/rosto; hashes protegidos, consumo atômico e rejeição de código inválido/usado ou replay concorrente são verificados | R02, R19 | Testado, incluindo consumo concorrente |
| AUTH14 | Recuperação de senha exige rosto + TOTP, sem envio de e-mail; falha não altera o hash, sucesso permite redefinir senha e não libera alteração de endereço/território | R02, R19, R28 | Testado |

## Sequência e validação

Após a fundação, implementar identidade sintética, sessão e políticas RBAC testáveis. Especificar e implementar bootstrap/setup e bloqueios antes das leis. Integrar o motor escolhido e recuperação real somente após corrigir/validar os fatores; ações de voto privado aguardam o protocolo.

Testes Rust, chamadas de rede não autorizadas, expiração/replay e E2E em navegador real estão em `src/application/identity/`, `src/server/identity_network_tests.rs` e `tests/e2e_identity.rs`; a configuração de produção rejeita o provider sintético (`src/infrastructure/config.rs`).
