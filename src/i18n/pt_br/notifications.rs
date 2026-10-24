pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.notifications" => "Notificações",
        "notifications.title" => "Notificações",
        "notifications.unread" => "não lidas",
        "notifications.mark_read" => "Marcar como lida",
        "notifications.mark_all" => "Marcar todas como lidas",
        "notifications.read" => "Lida",
        "notifications.open" => "Abrir",
        "notifications.empty" => "Você não tem notificações.",
        "notifications.older" => "Mais antigas",
        "notifications.newer" => "Mais recentes",
        "notifications.prefs.link" => "Preferências de notificação",
        "notifications.prefs.title" => "Preferências de notificação",
        "notifications.prefs.lead" => {
            "Escolha quais avisos aparecem no aplicativo. Avisos de segurança da conta são sempre enviados."
        }
        "notifications.prefs.category" => "Evento",
        "notifications.prefs.in_app" => "No aplicativo",
        "notifications.prefs.email" => "E-mail",
        "notifications.prefs.email_unavailable" => "indisponível nesta instância",
        "notifications.prefs.email_note" => {
            "Esta instância não tem remetente de e-mail configurado; nenhum e-mail é enviado."
        }
        "notifications.category.proposal_created" => "Proposta criada",
        "notifications.category.voting_opened" => "Votação aberta",
        "notifications.category.deadline_reminder" => "Lembrete de prazo",
        "notifications.category.result_published" => "Resultado publicado",
        "notifications.category.phase_changed" => "Mudança de fase",
        _ => return None,
    })
}
