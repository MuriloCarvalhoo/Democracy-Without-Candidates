pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.metalaws" => "Meta-leis",
        "nav.statistics" => "Estatísticas",
        "voting.title" => "Seu voto",
        "voting.demo_notice" => {
            "Votação demonstrativa: voto público vinculado à conta, sem sigilo, sem recibo criptográfico e sem validade eleitoral."
        }
        "voting.choice.legend" => "Opção",
        "voting.choice.yes" => "SIM",
        "voting.choice.no" => "NÃO",
        "voting.choice.abstain" => "ABSTENÇÃO",
        "voting.privacy.legend" => "Privacidade do voto",
        "voting.privacy.private" => "Privado (secreto) — indisponível",
        "voting.privacy.private_unavailable" => {
            "O voto secreto é o padrão pretendido, mas está bloqueado nesta instância: ainda não existe um protocolo eleitoral validado que garanta sigilo e unicidade. Sem o consentimento abaixo não é possível votar."
        }
        "voting.privacy.consent" => {
            "Entendo e aceito que este voto NÃO é secreto: ele fica registrado vinculado à minha conta, pode ser consultado por quem opera ou audita a instância e poderá ser divulgado publicamente. Não há recibo criptográfico."
        }
        "voting.continue" => "Continuar para confirmação",
        "voting.reauth_hint" => "Para registrar o voto, valide seu rosto e código.",
        "voting.sending" => "Enviando voto… aguarde a confirmação do servidor.",
        "voting.confirmed" => "Voto registrado pelo servidor:",
        "voting.already" => "Voto já registrado:",
        "voting.closed" => "A votação desta proposta não está aberta.",
        "voting.login" => "Entre na sua conta para votar.",
        "voting.ineligible" => {
            "Você não faz parte do eleitorado desta votação (fotografado na abertura)."
        }
        "voting.disabled" => "A votação está desabilitada nesta instância.",
        "voting.unknown" => {
            "Não foi possível confirmar o envio: o resultado é desconhecido. Verifique a situação antes de tentar de novo."
        }
        "voting.check" => "Verificar situação do voto",
        "voting.not_recorded" => {
            "Nenhum voto foi registrado para esta tentativa. Você pode tentar novamente."
        }
        "voting.electorate" => "Eleitorado",
        "voting.vote_link" => "Votar",
        "voting.error.consent" => {
            "É preciso consentir com o voto público: o voto secreto está indisponível."
        }
        "voting.error.choice" => "Escolha SIM, NÃO ou ABSTENÇÃO.",
        "voting.error.already_voted" => "Voto já registrado para esta proposta.",
        "voting.notify.opened" => "Votação aberta",
        "voting.notify.reminder" => "Faltam 24 horas para o fim da votação",
        "voting.notify.result" => "Resultado publicado",
        "voting.reason.quorum" => "participação abaixo do quórum",
        "voting.reason.tie" => "empate entre SIM e NÃO",
        "voting.reason.no_valid" => "nenhum voto válido (apenas abstenções)",
        "voting.reason.majority" => "maioria exigida não atingida",
        "voting.result.link" => "Resultado",
        "voting.result.not_opened" => "Esta proposta não chegou à votação.",
        "voting.result.open" => "Votação em andamento até",
        "voting.result.no_partials" => {
            "Parciais não são exibidas antes do encerramento, para não influenciar quem ainda vai votar."
        }
        "voting.result.closing" => "Prazo encerrado; apuração em processamento.",
        "voting.result.option" => "Opção",
        "voting.result.votes" => "Votos",
        "voting.result.share" => "Percentual",
        "voting.result.of_valid" => "dos válidos",
        "voting.result.of_participation" => "da participação",
        "voting.result.of_electorate" => "do eleitorado",
        "voting.result.snapshot" => "Fotografia do eleitorado",
        "voting.result.participation" => "Participação",
        "voting.result.quorum" => "Quórum necessário",
        "voting.result.reached" => "atingido",
        "voting.result.not_reached" => "não atingido",
        "voting.result.valid" => "Votos válidos (SIM + NÃO)",
        "voting.result.majority" => "Maioria necessária",
        "voting.result.closed_at" => "Encerrada em",
        "voting.result.package_adopted" => "Pacote de parâmetros adotado como versão",
        "voting.result.package_rejected" => "Pacote de parâmetros não adotado.",
        "voting.result.package_rejected_initial" => {
            "Pacote não adotado: leis continuam bloqueadas até um novo plebiscito aprovado."
        }
        "voting.result.rounding" => {
            "Quórum conta SIM+NÃO+ABSTENÇÃO: mínimo = ⌈pb × eleitorado / 10000⌉. Maioria usa só votos válidos: SIM ≥ ⌊pb × válidos / 10000⌋ + 1; empate ou zero válidos rejeita. Percentuais exibidos em pontos-base, arredondados ao 0,01% mais próximo (meio para cima); a decisão usa as contagens inteiras exatas."
        }
        "voting.setup.title" => "Plebiscito de configuração inicial",
        "voting.setup.adopted" => "Setup nacional vigente: versão",
        "voting.setup.open" => "Plebiscito em votação.",
        "voting.setup.not_started" => {
            "Plebiscito não iniciado. Nenhuma lei pode ser criada antes da adoção do setup."
        }
        "voting.setup.failed" => {
            "O último plebiscito não aprovou o pacote; leis continuam bloqueadas e um novo plebiscito pode ser iniciado."
        }
        "voting.setup.start" => "Iniciar plebiscito nacional",
        "voting.setup.start_hint" => {
            "O plebiscito usa o pacote sugerido abaixo. A administração apenas inicia: não edita parâmetros nem adota o resultado."
        }
        "voting.setup.suggested" => "Pacote sugerido",
        "voting.setup.readonly" => {
            "Somente leitura (pontos-base: 10000 = 100%; prazos em dias). Mudanças posteriores só por meta-lei."
        }
        "voting.setup.past" => "Plebiscitos anteriores",
        "voting.setup.plebiscite_title" => "[Demonstração] Plebiscito de configuração inicial",
        "voting.setup.plebiscite_body" => {
            "Plebiscito demonstrativo para adotar o pacote sugerido de parâmetros (PRD §2.3) como primeira versão do setup nacional. Aprovação exige quórum e maioria da configuração inicial; rejeição ou falta de quórum mantém as leis bloqueadas."
        }
        "voting.diff.title" => "Parâmetros: vigente × proposto",
        "voting.diff.current_version" => "Comparação com o setup vigente, versão",
        "voting.diff.no_current" => {
            "Sem setup vigente diferente para comparar (pacote já adotado ou inexistente)."
        }
        "voting.diff.param" => "Parâmetro",
        "voting.diff.current" => "Vigente",
        "voting.diff.proposed" => "Proposto",
        "voting.diff.note" => {
            "● indica valor alterado. Se aprovado, vale apenas para eleições abertas depois do resultado."
        }
        "metalaws.title" => "Meta-leis",
        "metalaws.new" => "Nova meta-lei",
        "metalaws.hint" => {
            "Alterações do setup seguem coleta, debate e votação de 30 dias com quórum e maioria especiais."
        }
        "metalaws.new_hint" => {
            "Os parâmetros vêm preenchidos com o setup vigente; o servidor valida cada faixa."
        }
        "metalaws.empty" => "Nenhuma meta-lei publicada.",
        "metalaws.diff_link" => "Ver parâmetros vigentes × propostos",
        "stats.title" => "Estatísticas",
        "stats.by_status" => "Propostas por situação",
        "stats.by_kind" => "Propostas por tipo",
        "stats.closed_votes" => "Votações encerradas",
        "stats.citizens" => "Cidadãos ativos por estado",
        "stats.min_cell" => {
            "Contagens abaixo do mínimo são ocultadas para não identificar pessoas; mínimo por célula:"
        }
        "stats.total" => "Total",
        "stats.suppressed" => "omitido (revelaria uma célula pequena)",
        _ => return None,
    })
}
