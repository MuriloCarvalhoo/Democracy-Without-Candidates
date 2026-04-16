# Democracy-Without-Candidates

Autenticação biométrica fácil e única (ex: facial + liveness detection ou impressão digital via app/dispositivo compatível).
Perfil do usuário: opção de público ou privado (nome, foto, bio, histórico de propostas — visível ou não para outros).
Votos: cada eleitor escolhe se seu voto é público (nome + voto visível, para accountability) ou privado (apenas o voto conta no total, sem identificação individual).
Configuração por país: o setup inicial (separação de competências entre níveis, percentuais de assinaturas, quorum mínimo, limites de propostas, etc.) é definido por votação direta inicial (plebiscito de adoção).
Qualquer alteração no setup (incluindo quorum, percentuais, regras de urgência, etc.) exige votação direta com regras especiais (quorum mais alto + maioria qualificada).

O sistema é projetado para ser plug-and-play por país: ao adotar a plataforma, o primeiro passo é uma votação de configuração inicial (com parâmetros padrão sugeridos, mas alteráveis só por voto).
1. Autenticação e Perfil do Usuário

Autenticação única: obrigatório biometria (facial com detecção de vida + documento oficial escaneado ou integrado via gov API). Garante 1 pessoa = 1 conta irreversível (hash biométrico armazenado de forma segura, não a imagem raw).
Perfil: ao criar conta, o usuário escolhe:
Público: nome real (ou pseudônimo verificado), foto, bio, histórico de propostas e votos (se optar por voto público) visíveis para todos.
Privado: só CPF/ID interno visível para sistema (anonimato relativo); outros veem só "usuário anônimo" ou ID numérico.

Opção de mudar privacidade a qualquer momento (mas votos já emitidos mantêm configuração original).

2. Privacidade dos Votos

Cada eleitor, ao votar em uma proposta, escolhe:
Voto público: nome/perfil + voto ("Sim/Não/Abstenção") registrado publicamente (aumenta transparência, útil para accountability em cargos ou debates).
Voto privado: só o voto entra na contagem total; ninguém (nem sistema) associa ao eleitor depois (criptografia zero-knowledge ou similar para provar validade sem revelar identidade).

Padrão inicial: privado (para incentivar participação sem medo), mas opt-in para público.
Estatísticas agregadas sempre públicas (ex: % por região, demografia se autorizado).

3. Configuração Inicial e Alterações no Setup (Meta-regras)

Ao adotar a plataforma em um país (ou estado/cidade), o primeiro passo obrigatório é uma votação de configuração inicial (plebiscito de setup).
Parâmetros a votar:
Separação de competências (o que é nacional/estadual/municipal).
Percentuais de assinaturas para proposição.
Quorum mínimo para normais e urgentes.
Limites de propostas por ano.
Regras de urgência (prazo, justificativa).
Outros (ex: período de debate, resumo obrigatório, etc.).

Votação inicial: quorum alto (ex: 40% nacional) + maioria qualificada (60% +1 dos válidos).
Parâmetros padrão sugeridos (pré-carregados, mas podem ser alterados na votação):
Assinaturas: 0,5% nacional / 1% estadual / 2% municipal.
Quorum normais: 25%.
Quorum urgentes: 20%.
Prazo normais: coleta 90 dias + debate 15 + votação 30 dias.
Prazo urgentes: coleta 15 dias + votação 7 dias.


Qualquer alteração futura no setup: só por proposta de alteração do setup (tratada como lei constitucional/meta-lei).
Requisitos mais rígidos: quorum mínimo 35% + maioria 60% +1 (para evitar mudanças impulsivas nas regras do jogo).
Propostas de setup vão direto para votação especial (não misturadas com leis comuns).


4. Processo de Leis (resumo mantido, com quorum mínimo)

Leis normais: coleta 90 dias → debate 15 dias → votação 30 dias → quorum 25% + maioria simples.
Leis urgentes: coleta 15 dias → votação 7 dias → quorum 20% + maioria simples.
Limite: 5 normais + 1 urgente por ano por cidadão.
Sem censura prévia.

5. Implementação Técnica Geral

Plataforma neutra, open-source ou auditável.
Blockchain ou tecnologia similar para imutabilidade de votos e resultados.
Integração com sistemas gov existentes para biometria (ex: gov.br, DNI eletrônico, etc.).
App/web + suporte offline em pontos físicos se necessário.
