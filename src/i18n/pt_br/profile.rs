//! Profile and privacy settings.

pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "profile.title" => "Meu perfil",
        "profile.data" => "Dados",
        "profile.saved" => "Alteração salva.",
        "profile.pending" => "Cadastro pendente: conclua a validação para participar.",
        "profile.finish_enrollment" => "Concluir validação",
        "profile.reenroll_totp" => {
            "Você entrou com um código de recuperação. Recadastre o autenticador antes de ações críticas."
        }
        "profile.reenroll_link" => "Recadastrar autenticador",
        "profile.address.title" => "Endereço (privado)",
        "profile.address_locked" => "O endereço cadastrado não pode ser alterado nesta versão.",
        "profile.address.street" => "Logradouro",
        "profile.address.neighborhood" => "Bairro",
        "profile.address.city" => "Município",
        "profile.address.country" => "País",
        "profile.stats" => "Participação",
        "profile.stats_empty" => "Estatísticas de participação aparecerão aqui.",
        "profile.security" => "Privacidade e segurança",
        "profile.privacy_link" => "Configurações de privacidade",
        "profile.privacy.title" => "Configurações de privacidade",
        "profile.visibility.profile" => "Visibilidade do perfil",
        "profile.visibility.vote" => "Preferência padrão de voto",
        "profile.visibility.vote_hint" => "Usada como padrão quando houver votação com essa opção.",
        "profile.visibility.public" => "Público",
        "profile.visibility.private" => "Privado",
        _ => return None,
    })
}
