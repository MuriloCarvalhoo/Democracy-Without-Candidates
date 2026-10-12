# Identidade, sessão e autorização

Atualizado em 2026-10-12. Desenho para Rust/Leptos/Axum; nenhuma autenticação ou integração biométrica implementada. Requisitos no [PRD](../prd/prd.md), decisões e bloqueadores no [plano SDD](../development/sdd-plan.md).

## Limites do desenho

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

1. Receber dados mínimos do cadastro e validar formato, consentimentos aplicáveis e critérios de elegibilidade ainda a especificar.
2. Criar identidade pendente, tratando duplicidade sem expor documentos ou revelar contas de terceiros.
3. Executar prova de identidade pelo provider, com falhas, timeout e bloqueio de tentativas definidos.
4. Configurar segundo fator e política de recuperação conforme o fluxo aprovado.
5. Ativar CITIZEN apenas após as verificações necessárias; fixtures sintéticas permanecem rotuladas como teste.

Elegibilidade e vínculo territorial dependem das decisões do PRD. Não persistir imagem facial bruta; forma de referência/template protegido depende do provedor. Não usar igualdade de um hash como especificação de matching biométrico.

## Login, sessão e recuperação

Senha versus fluxo sem senha permanece decisão aberta no PRD.

Definir o contrato de sessão antes de implementar: credenciais, armazenamento, expiração, renovação, revogação, logout, proteção de requisições e recuperação após perda de dispositivo/TOTP. O formato de sessão ainda será decidido. Não incluir documentos, material biométrico, segredo TOTP ou dados eleitorais privados em tokens ou payloads públicos.

Segredos do segundo fator exigem proteção e gestão de chaves especificadas. Definir replay de TOTP, limite de tentativas, troca de dispositivo e recuperação; não adiar recuperação enquanto se promete acesso de produção completo.

Estados da interface: visitante, cadastro pendente, autenticado, reautenticação necessária, sessão expirada, acesso negado e conta bloqueada. Renovação de sessão não equivale a reautenticação de uma ação crítica.

## Reautenticação e autorização no servidor

Ações críticas previstas: criar proposta, assinar, votar, alterar dados sensíveis/privacidade e participar de alterações do setup. A validade, o vínculo a recurso/ação e o consumo/replay da prova de reautenticação devem ser especificados; o navegador não pode declarar que a verificação ocorreu.

| Camada | Verificação |
| --- | --- |
| Endpoint Leptos/Axum | Entrada limitada, sessão obtida no servidor e proteção de transporte |
| Aplicação | Status, permissão, autoria, jurisdição, fase, setup e reautenticação |
| Domínio/persistência | Invariantes, duplicidade, quotas e efeito transacional |
| Interface | Apresentação das condições e tratamento de erros; sem autoridade final |

Para voto, revalidar elegibilidade e fase junto à aceitação, inclusive disputa com encerramento. Para assinatura, exigir fase de coleta e unicidade. Para criação/publicação, exigir autoria, limite aplicável e setup adotado. O momento de consumo da quota por rascunho/publicação permanece aberto.

Não vincular automaticamente a cédula privada a registro de reautenticação identificável. O protocolo precisa coordenar elegibilidade, prova de ação e unicidade sem contradizer a promessa de sigilo.

## Auditoria e erros

Eventos relevantes: cadastro, login/falha, verificação de identidade, segundo fator, renovação/revogação de sessão, reautenticação e acesso negado. Definir dados mínimos, separação de logs operacionais e evidências públicas e retenção por categoria.

Mensagens devem permitir corrigir credenciais, sessão expirada e reautenticação sem revelar que determinado documento/biometria pertence a outra pessoa. Registrar falha de provider ou banco sem transformar erro em cidadão validado. Não expor biometria, segredos, documentos ou associação com voto privado.

## Critérios de aceitação pendentes

Os IDs AUTH01–AUTH12 são estáveis. A coluna PRD relaciona critérios aos [requisitos do produto](../prd/prd.md#12-critérios-de-aceite); um critério isolado não encerra esses requisitos. Todos aguardam implementação e evidência.

| ID | Critério futuro e evidência necessária | PRD relacionado |
| --- | --- | --- |
| AUTH01 | Cadastro válido funciona; entradas inválidas e falhas do provider não ativam a conta | R02 |
| AUTH02 | Cadastro inicia em PENDING_CITIZEN e não permite ações eleitorais | R02, R03 |
| AUTH03 | Só verificações aprovadas permitem CITIZEN; teste sintético não valida provider real | R02, R03 |
| AUTH04 | Login estabelece sessão válida conforme contrato definido; falha não estabelece sessão | R02, R19 |
| AUTH05 | Sessão/payloads públicos não contêm dados sensíveis; expiração e revogação são testadas | R19, RNF10 |
| AUTH06 | RBAC nega chamadas de rede protegidas sem autorização | R03, R19 |
| AUTH07 | Permissão é verificada com escopo, autoria e condições do recurso | R03, R13, R19 |
| AUTH08 | Ação crítica rejeita reautenticação ausente, inválida, expirada ou para outra ação | R09, R11, R19 |
| AUTH09 | Voto exige identidade/elegibilidade e face + 2FA conforme provider/protocolo aprovados | R02, R05, R11 |
| AUTH10 | Assinatura exige reautenticação prevista e não aceita duplicidade | R09, R19 |
| AUTH11 | Criar proposta exige reautenticação, setup e limites aplicáveis | R06, R14, R19 |
| AUTH12 | Eventos relevantes são auditáveis sem expor segredos ou comprometer voto privado | R20, R21, RNF10 |

## Sequência e validação

Após a fundação, implementar identidade sintética, sessão e políticas RBAC testáveis. Especificar e implementar bootstrap/setup e bloqueios antes das leis. Integrar identidade real e recuperação somente após definição do provider; ações de voto privado aguardam o protocolo.

Executar testes Rust, chamadas de rede não autorizadas, cenários de expiração/replay e navegador real. Testar explicitamente que configuração de produção rejeita mock. Ainda não há evidência de nenhum desses critérios.
