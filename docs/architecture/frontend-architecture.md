# Arquitetura da interface web

Atualizado em 2026-10-12. Interface aprovada: Leptos em Rust, SSR e hidratação WASM. Ainda não há telas executáveis. Ver [PRD](../prd/prd.md), [wireframes](../wireframes/wireframes.md) e [plano SDD](../development/sdd-plan.md).

## Renderização e estado

O servidor entrega HTML inicial; a hidratação WASM conecta as interações no navegador. HTML/CSS e JavaScript gerado para carregar o WASM são artefatos necessários da web, sem alterar a decisão por lógica Rust.

Organizar telas e server functions por feature dentro do mesmo pacote. Usar os mecanismos de estado e composição do Leptos conforme a versão validada no scaffold. Compartilhar apenas tipos públicos; banco, biometria e segredos do servidor não entram no bundle do navegador.

Manter estado local de formulários e feedback; promover a estado compartilhado somente sessão pública mínima, navegação, tema e preferências que realmente precisem ser compartilhadas. Resultados, elegibilidade e permissões dependem da resposta do servidor.

## Rotas e componentes

O [mapa de telas do PRD](../prd/prd.md#11-mapa-de-telas-31-telas) é a referência para rotas de produto. Não manter uma segunda lista incompatível de rotas traduzidas. Telas de administração global estão propostas para expansão e não integram a fundação de uma instância por país.

| Grupo | Componentes e responsabilidade |
| --- | --- |
| Layout | Navegação pública, autenticação, área do cidadão e administração local |
| Elementos reutilizáveis | Botão, campo, cartão, badge, tabela, modal e diálogo |
| Feedback | Carregamento, estado vazio, erro, confirmação e progresso |
| Features | Propostas, assinaturas, debate, setup, perfil, votação e auditoria |
| Visualização | Estatísticas agregadas e resultados com apresentação acessível |

Não criar design system ou pacotes independentes antecipadamente. Proposta para revisão: CSS simples e componentes locais, sem biblioteca de estilo obrigatória.

## Transporte e segurança

Formulários e ações usam inicialmente server functions Leptos, delegando às regras da aplicação. Sucesso de transporte não basta: apresentar o resultado confirmado da operação de domínio.

Ocultar botões e redirecionar rotas melhora a experiência, mas o servidor deve revalidar cada chamada. Não aceitar permissões, país ou confirmação biométrica apenas do estado do navegador. A sessão fica no servidor e o navegador usa cookie de sessão HttpOnly, Secure em produção HTTPS, conforme o [guia de autenticação](auth-rbac-flow.md). Não armazenar JWT/identificador de sessão em localStorage nem tentar ler o cookie por JavaScript.

Login identifica a conta por e-mail e exige senha, rosto e TOTP. Cadastro exige e-mail, senha, nome e endereço, sem documento nacional. Após gravação inicial, endereço é exibido somente para leitura; o servidor também rejeita alterações. Não oferecer “manter conectado” nem persistir credenciais de login entre usos. Após 15 minutos sem atividade, a sessão expira no servidor e a interface solicita novo login, sem reaplicar ações críticas silenciosamente. Não persistir senha, material biométrico, segredos TOTP ou votos privados em armazenamento local. Proteger renderização de conteúdo de usuários e definir políticas de sessão, CSRF e conteúdo antes das features afetadas. Reautenticação deve ter validade e vínculo à ação definidos no servidor.

## Preenchimento de endereço

No cadastro brasileiro, informar CEP e buscar pelo servidor usando o provedor habilitado. Exibir `state`, `city`, `neighborhood` e `street` preenchidos automaticamente; país identificado pela instância. `service` e `ibge` são dados de persistência, sem campos editáveis de formulário. Não oferecer campos de número ou complemento nesta versão.

Mostrar busca em curso, CEP inválido/não encontrado e serviço indisponível. Se o CEP mudar antes do cadastro, invalidar o resultado anterior e ignorar respostas antigas; não preencher com a resposta de outro CEP. Sem provedor habilitado ou sem território resolvido, não anunciar endereço confirmado. Após o cadastro, manter o bloqueio de alteração.

## Estados observáveis

Funcionalidades desativadas por flags da instância não oferecem ações na interface. O estado vem do servidor, que também rejeita chamadas diretas. Se a configuração mudar após reinício entre exibição e envio, apresentar indisponibilidade conforme o estado efetivo definido no [guia do servidor](backend-architecture.md#feature-flags-por-instância).

| Situação | Comportamento a especificar/testar |
| --- | --- |
| Carregando | Indicar operação em curso; preservar contexto e evitar submissão acidental |
| Vazio | Diferenciar ausência de dados de falha ao buscar dados |
| Erro de validação | Identificar campo e correção com texto acessível |
| Sucesso | Mostrar confirmação só após aceitação pelo servidor |
| Sessão expirada | Solicitar login/reautenticação sem reaplicar silenciosamente ação crítica |
| Acesso negado | Informar indisponibilidade de ação sem expor dados restritos |
| Timeout após envio | Informar resultado ainda desconhecido; consultar/repetir conforme idempotência definida |
| Câmera negada/incompatível | Explicar falha e caminho permitido pelo provedor, sem transformar falha em validação |

Nos fluxos eleitorais distinguir encerramento, inelegibilidade, voto já aceito, replay e falha interna. A mensagem “voto registrado” depende de persistência confirmada. Especificar quando rascunhos podem ser recuperados sem armazenar dados sensíveis indevidamente.

## Mobile, acessibilidade e idiomas

Manter mobile-first, navegação por teclado, HTML semântico, labels, foco em diálogos, contraste e feedback para leitores de tela. O alvo de produto permanece WCAG 2.1 AA; conformidade depende de inspeção e testes reais, não da escolha do framework.

Preparar textos e formatação para i18n. Proposta: pt-BR na versão inicial, estrutura pronta para i18n e inglês/espanhol preservados como expansão, conforme o SDD. Horários exibidos devem permitir compreender o prazo aplicável e não redefinir o encerramento do servidor.

## PWA e conexão

Instalabilidade permanece requisito de produto; PWA não integra o critério mínimo da fundação. A aplicação funciona exclusivamente online, inclusive quando instalada.

Sem conexão, indicar indisponibilidade e permitir nova tentativa quando a conexão retornar, sem fila local de assinaturas ou votos. Ações críticas só são confirmadas após aceitação persistida no servidor. Não oferecer votação ou coleta de assinaturas em meio físico. Cache técnico de assets não constitui modo offline; definir atualização e limpeza de dados sensíveis. Push notifications são evolução futura; in-app/e-mail continuam no PRD.

## Validação e performance

Compilar separadamente servidor e WASM. Na primeira fatia, executar uma interação real após hidratação e uma regressão Rust com Fantoccini/WebDriver. Chrome DevTools MCP complementa essa regressão com inspeção de console, rede, mobile e acessibilidade.

Testes nativos não validam foco, câmera, hidratação ou cache no navegador. Novas telas devem ter casos de carregamento, erro, sessão e permissões, além do caminho feliz.

Medir tamanho do WASM/assets, tempo de carregamento e custo das consultas antes de otimizações. Paginação e consultas limitadas vêm do servidor; cache não deve mascarar mudança de fase eleitoral.
