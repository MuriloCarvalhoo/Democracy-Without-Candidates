pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.proposals" => "Propostas",
        "nav.dashboard" => "Painel",
        "nav.setup_history" => "Setup",
        "proposals.kind.normal_law" => "Lei normal",
        "proposals.kind.urgent_law" => "Lei urgente",
        "proposals.kind.setup_change" => "Alteração do setup (meta-lei)",
        "proposals.kind.initial_setup" => "Plebiscito de configuração inicial",
        "proposals.level.national" => "Nacional",
        "proposals.level.state" => "Estadual",
        "proposals.level.municipal" => "Municipal",
        "proposals.status.draft" => "Rascunho",
        "proposals.status.collecting_signatures" => "Coleta de assinaturas",
        "proposals.status.in_debate" => "Em debate",
        "proposals.status.in_voting" => "Em votação",
        "proposals.status.approved" => "Aprovada",
        "proposals.status.rejected" => "Rejeitada",
        "proposals.status.quorum_not_reached" => "Quórum não atingido",
        "proposals.status.expired" => "Expirada",
        "proposals.status.archived" => "Arquivada",
        "proposals.list.title" => "Propostas",
        "proposals.list.new" => "Nova proposta",
        "proposals.list.empty" => "Nenhuma proposta publicada ainda.",
        "proposals.list.no_match" => "Nenhuma proposta corresponde aos filtros.",
        "proposals.list.total" => "propostas encontradas",
        "proposals.filter.any" => "Todos",
        "proposals.filter.apply" => "Filtrar",
        "proposals.filter.clear" => "Limpar filtros",
        "proposals.page.prev" => "Anterior",
        "proposals.page.next" => "Próxima",
        "proposals.page.of" => "Página",
        "proposals.field.kind" => "Tipo",
        "proposals.field.level" => "Nível",
        "proposals.field.status" => "Situação",
        "proposals.field.title" => "Título",
        "proposals.field.summary" => "Resumo",
        "proposals.field.body" => "Texto completo",
        "proposals.field.jurisdiction" => "Jurisdição",
        "proposals.field.author" => "Autoria",
        "proposals.field.published_at" => "Publicada em",
        "proposals.field.setup_version" => "Versão do setup",
        "proposals.field.text_hash" => "SHA-256 do texto publicado",
        "proposals.field.archive_reason" => "Motivo do arquivamento",
        "proposals.field.reason" => "Motivo",
        "proposals.signatures" => "Assinaturas",
        "proposals.signatures.of" => "de",
        "proposals.signatures.target_reached" => "Meta de assinaturas atingida.",
        "proposals.deadline" => "Prazo",
        "proposals.deadline.collect" => "Fim da coleta de assinaturas",
        "proposals.deadline.debate" => "Fim do debate",
        "proposals.deadline.vote" => "Fim da votação",
        "proposals.deadline.passed" => "encerrado",
        "proposals.timezone_note" => "Horários de Brasília (America/Sao_Paulo).",
        "proposals.action.sign" => "Assinar",
        "proposals.action.unsign" => "Retirar assinatura",
        "proposals.action.edit" => "Editar rascunho",
        "proposals.action.publish" => "Publicar",
        "proposals.action.withdraw" => "Retirar proposta",
        "proposals.action.archive" => "Arquivar",
        "proposals.action.export" => "Exportar texto (Markdown)",
        "proposals.action.signers" => "Ver assinantes",
        "proposals.action.save" => "Salvar rascunho",
        "proposals.action.confirm" => "Confirmar",
        "proposals.signed" => "Você assinou esta proposta.",
        "proposals.unsigned" => "Assinatura retirada.",
        "proposals.published" => "Proposta publicada. A coleta de assinaturas começou.",
        "proposals.withdrawn" => "Proposta retirada e arquivada.",
        "proposals.archived_ok" => "Proposta arquivada.",
        "proposals.saved" => "Rascunho salvo.",
        "proposals.not_eligible" => "Você não faz parte do eleitorado desta proposta.",
        "proposals.login_to_act" => "Entre na sua conta para assinar ou propor.",
        "proposals.closed_phase" => "Esta proposta está encerrada.",
        "proposals.archived_notice" => "Esta proposta foi arquivada.",
        "proposals.reauth_hint" => "Para confirmar, valide seu rosto e código.",
        "proposals.draft_notice" => {
            "Rascunho visível apenas para você. Rascunhos não consomem a cota anual."
        }
        "proposals.publish_hint" => {
            "Publicar consome a cota anual (cancelamento não devolve) e torna o texto imutável."
        }
        "proposals.duplicates" => "Atenção: existem propostas com título ou texto semelhante:",
        "proposals.new.title" => "Nova proposta",
        "proposals.edit.title" => "Editar rascunho",
        "proposals.new.level_hint" => "A jurisdição é definida pelo seu endereço cadastrado.",
        "proposals.new.setup_params" => {
            "Parâmetros propostos (pontos-base: 10000 = 100%; prazos em dias)"
        }
        "proposals.signers.title" => "Assinantes",
        "proposals.signers.count" => "Assinaturas ativas",
        "proposals.signers.private" => {
            "A lista nominal só mostrará perfis públicos; a visibilidade de perfil ainda não está disponível, por isso apenas a contagem é exibida."
        }
        "proposals.back" => "Voltar à proposta",
        "proposals.proposed_setup" => "Pacote de parâmetros em votação",
        "proposals.export.hash_rule" => {
            "Hash calculado sobre título, linha em branco, resumo, linha em branco e texto completo (UTF-8)."
        }
        "proposals.notify.published" => "Sua proposta foi publicada",
        "proposals.notify.phase" => "Sua proposta mudou de fase",
        "proposals.notify.archived" => "Sua proposta foi arquivada",
        "proposals.initial_setup.summary" => {
            "Plebiscito para adotar a configuração inicial de regras desta instância."
        }
        "proposals.error.length" => "Tamanho inválido",
        "proposals.error.filter" => "Filtro inválido",
        "proposals.error.setup_param" => "Parâmetro de setup fora do intervalo permitido.",
        "proposals.error.no_setup" => {
            "Nenhuma lei pode ser criada antes da adoção do setup desta jurisdição."
        }
        "proposals.error.setup_exists" => "O setup inicial já foi adotado.",
        "proposals.error.setup_in_progress" => "Já existe um plebiscito de setup em votação.",
        "proposals.error.quota" => "Limite anual de propostas deste tipo atingido.",
        "proposals.error.empty_electorate" => "Eleitorado vazio: a fase não pode ser aberta.",
        "proposals.error.already_signed" => "Você já assinou esta proposta.",
        "proposals.error.not_signed" => "Você não tem assinatura ativa nesta proposta.",
        "proposals.error.idempotency_conflict" => {
            "Esta tentativa já foi usada com outros dados. Recarregue a página."
        }
        "dashboard.title" => "Painel",
        "dashboard.voting" => "Votações ativas",
        "dashboard.debate" => "Propostas em debate",
        "dashboard.collecting" => "Coleta de assinaturas",
        "dashboard.mine" => "Minhas propostas",
        "dashboard.signed" => "Minhas assinaturas",
        "setup.history.title" => "Histórico do setup",
        "setup.history.empty" => {
            "Nenhum setup adotado. Leis só podem ser criadas após o plebiscito de configuração inicial."
        }
        "setup.history.version" => "Versão",
        "setup.history.adopted_at" => "Adotado em",
        "setup.history.synthetic" => "Sintético (teste)",
        "setup.history.source" => "Proposta de origem",
        "setup.history.inherit_note" => {
            "Estados e municípios herdam o setup nacional até adotarem o próprio. Mudanças valem apenas para eleições futuras."
        }
        "setup.status.adopted" => "Vigente",
        "setup.status.superseded" => "Substituído",
        "signature_bp_national" => "Assinaturas — nacional (pb)",
        "signature_bp_state" => "Assinaturas — estadual (pb)",
        "signature_bp_municipal" => "Assinaturas — municipal (pb)",
        "normal_quorum_bp" => "Lei normal — quórum (pb)",
        "normal_majority_bp" => "Lei normal — maioria (pb)",
        "normal_collect_days" => "Lei normal — coleta (dias)",
        "normal_debate_days" => "Lei normal — debate (dias)",
        "normal_vote_days" => "Lei normal — votação (dias)",
        "urgent_quorum_bp" => "Lei urgente — quórum (pb)",
        "urgent_majority_bp" => "Lei urgente — maioria (pb)",
        "urgent_collect_days" => "Lei urgente — coleta (dias)",
        "urgent_vote_days" => "Lei urgente — votação (dias)",
        "setup_quorum_bp" => "Meta-lei — quórum (pb)",
        "setup_majority_bp" => "Meta-lei — maioria (pb)",
        "setup_collect_days" => "Meta-lei — coleta (dias)",
        "setup_debate_days" => "Meta-lei — debate (dias)",
        "setup_vote_days" => "Meta-lei — votação (dias)",
        "initial_quorum_bp" => "Configuração inicial — quórum (pb)",
        "initial_majority_bp" => "Configuração inicial — maioria (pb)",
        "initial_vote_days" => "Configuração inicial — votação (dias)",
        "quota_normal_per_year" => "Cota anual — leis normais",
        "quota_urgent_per_year" => "Cota anual — leis urgentes",
        _ => return None,
    })
}
