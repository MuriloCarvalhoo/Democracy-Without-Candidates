# Identidade, sessão e autorização

Atualizado em 2026-10-12. Desenho para Rust/Leptos/Axum; nenhuma autenticação ou integração biométrica implementada. Requisitos no [PRD](../prd/prd.md), decisões e bloqueadores no [plano SDD](../development/sdd-plan.md).

## Limites do desenho

E-mail é o identificador obrigatório de login. Cadastro exige e-mail, senha, nome e endereço, sem CPF ou outro documento nacional. Login exige senha, verificação facial e TOTP; informar um e-mail não comprova controle da conta nem unicidade de pessoa.

O produto pretende comprovar identidade e impedir contas/votos duplicados, com biometria facial e segundo fator TOTP. As decisões de identidade e sessão estão no [PRD, seção 13](../prd/prd.md#13-decisões-de-produto-pendentes). Biometria e 2FA não são, por si, evidência de unicidade eleitoral.

Provider de teste permite cenários determinísticos, nunca comprova identidade real e deve ser explicitamente habilitado e rejeitado em produção.

## Papéis e limites

| Papel | Acesso pretendido |
| --- | --- |
| VISITOR | Leitura pública de propostas, resultados e estatísticas |
| PENDING_CITIZEN | Completar cadastro e validação, sem participação eleitoral |
| CITIZEN | Perfil, propostas, assinaturas, votação e comentários conforme elegibilidade |
| AUDITOR | Evidências autorizadas somente para leitura e dentro do escopo credenciado |
| COUNTRY_ADMIN | Operação da instância e gestão territorial autorizada; sem alterar regras eleitorais diretamente |
| MASTER_ADMIN | Funções de plataforma previstas no PRD; autoridade global e implantação ainda a revisar |

Admin Master permanece na expansão pretendida; não é requisito da instância inicial. Não criar acesso global, banco global ou aprovação de cidadão pelo simples recebimento de um nome de role do cliente. Bootstrap de administradores e credenciamento de auditores dependem das decisões do PRD.

Permissões seguem `recurso:ação`, por exemplo `proposal:create`, `proposal:sign`, `proposal:vote`, `profile:update` e `audit:read`. Escopo territorial, autoria e fase da proposta também são condições obrigatórias. Uma permissão nominal não autoriza um cidadão inelegível a votar.

Administradores podem executar tarefas operacionais autorizadas; alteração de parâmetros eleitorais exige a meta-lei aprovada prevista em R07. O bootstrap do primeiro plebiscito permanece a especificar.

## Cadastro e status

1. Receber e-mail, senha, nome e endereço obrigatórios, sem documento nacional; validar formato, consentimentos aplicáveis e critérios de elegibilidade ainda a especificar.
2. Criar identidade pendente, tratando duplicidade sem revelar contas de terceiros.
3. Executar prova de identidade pelo provider, com falhas, timeout e bloqueio de tentativas definidos.
4. Configurar segundo fator e política de recuperação conforme o fluxo aprovado.
5. Ativar CITIZEN apenas após as verificações necessárias; fixtures sintéticas permanecem rotuladas como teste.

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

Os IDs AUTH01–AUTH14 são estáveis. A coluna PRD relaciona critérios aos [requisitos do produto](../prd/prd.md#12-critérios-de-aceite); um critério isolado não encerra esses requisitos. Todos aguardam implementação e evidência.

| ID | Critério futuro e evidência necessária | PRD relacionado |
| --- | --- | --- |
| AUTH01 | Cadastro exige e-mail, senha, nome e endereço; campos obrigatórios ausentes/inválidos ou falhas do provider não ativam a conta | R02, R28, R29 |
| AUTH02 | Cadastro inicia em PENDING_CITIZEN e não permite ações eleitorais | R02, R03 |
| AUTH03 | Ativação não exige confirmação de e-mail, mas exige as demais verificações aprovadas; falhas não ativam CITIZEN e teste sintético não valida provider real | R02, R03 |
| AUTH04 | Login usa e-mail + senha, rosto e TOTP, ou código válido no fluxo de recuperação de TOTP; falha em senha/rosto ou segundo fator sem recuperação válida não estabelece sessão | R02, R19 |
| AUTH05 | Sessão persiste no TursoDB e mantém expiração/revogação após reinício, com cookie opaco HttpOnly/Secure em produção e isolado por host; payloads públicos não contêm dados sensíveis; não há manter conectado ou persistência de login entre usos; expiração por 15 minutos de inatividade, revogação e reabertura/restauração do navegador são testadas | R19, RNF10 |
| AUTH06 | RBAC nega chamadas de rede protegidas sem autorização | R03, R19 |
| AUTH07 | Permissão considera o território derivado do endereço cadastrado, autoria e condições do recurso; endereço não pode ser alterado após cadastro | R03, R13, R19, R28 |
| AUTH08 | Ação crítica rejeita reautenticação ausente, inválida, expirada ou para outra ação | R09, R11, R19 |
| AUTH09 | Voto exige identidade/elegibilidade e face + 2FA conforme provider/protocolo aprovados | R02, R05, R11 |
| AUTH10 | Assinatura exige reautenticação prevista e não aceita duplicidade | R09, R19 |
| AUTH11 | Criar proposta exige reautenticação, setup e limites aplicáveis | R06, R14, R19 |
| AUTH12 | Eventos relevantes são auditáveis sem expor segredos ou comprometer voto privado | R20, R21, RNF10 |
| AUTH13 | Recuperação de TOTP usa código de uso único, mantendo senha/rosto; hashes protegidos, consumo atômico e rejeição de código inválido/usado ou replay concorrente são verificados | R02, R19 |
| AUTH14 | Recuperação de senha exige rosto + TOTP, sem envio de e-mail; falha não altera o hash, sucesso permite redefinir senha e não libera alteração de endereço/território | R02, R19, R28 |

## Sequência e validação

Após a fundação, implementar identidade sintética, sessão e políticas RBAC testáveis. Especificar e implementar bootstrap/setup e bloqueios antes das leis. Integrar o motor escolhido e recuperação real somente após corrigir/validar os fatores; ações de voto privado aguardam o protocolo.

Executar testes Rust, chamadas de rede não autorizadas, cenários de expiração/replay e navegador real. Testar explicitamente que configuração de produção rejeita mock. Ainda não há evidência de nenhum desses critérios.
