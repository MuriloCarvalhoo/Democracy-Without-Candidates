pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "app.name" => "Democracia Sem Candidatos",
        "app.synthetic_banner" => "Ambiente de teste: dados sintéticos, sem validade eleitoral.",
        "nav.home" => "Início",
        "nav.main" => "Navegação principal",
        "common.loading" => "Carregando…",
        "common.retry" => "Tentar novamente",
        "common.empty" => "Nada por aqui ainda.",
        "common.save" => "Salvar",
        "common.cancel" => "Cancelar",
        "common.offline" => {
            "Sem conexão. Nenhuma ação foi enviada; tente novamente quando a conexão voltar."
        }
        "error.unauthenticated" => "Entre na sua conta para continuar.",
        "error.session_expired" => "Sua sessão expirou. Entre novamente.",
        "error.forbidden" => "Você não tem permissão para esta ação.",
        "error.not_found" => "Não encontrado.",
        "error.rate_limited" => "Muitas tentativas. Aguarde alguns minutos.",
        "error.feature_disabled" => "Funcionalidade indisponível nesta instância.",
        "error.reauth_required" => "Confirme sua identidade (rosto e código) para esta ação.",
        "error.phase_closed" => "Esta etapa está encerrada.",
        "error.internal" => "Erro interno. Nada foi confirmado; tente novamente.",
        "error.transport" => {
            "Não foi possível falar com o servidor. O resultado é desconhecido; verifique antes de repetir."
        }
        "notfound.title" => "Página não encontrada",
        "home.title" => "Democracia direta, sem candidatos",
        "home.lead" => {
            "Proponha, debata e vote decisões coletivas com regras definidas por plebiscito."
        }
        "foundation.counter" => "Contador sintético persistido",
        "foundation.increment" => "Incrementar",
        "foundation.confirmed" => "Valor confirmado pelo servidor",
        _ => return None,
    })
}
