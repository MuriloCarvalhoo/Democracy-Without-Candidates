pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.admin" => "Administração",
        "admin.title" => "Administração da instância",
        "admin.nav" => "Seções da administração",
        "admin.nav.instance" => "Flags e instância",
        "admin.nav.iam" => "Permissões (IAM)",
        "admin.flags.title" => "Funcionalidades (feature flags)",
        "admin.flags.hint" => {
            "Mudanças ficam gravadas e só valem depois que o operador reiniciar a aplicação na VPS. Este painel não reinicia o serviço."
        }
        "admin.flags.restart_pending" => {
            "Há mudanças gravadas pendentes de reinício. O estado efetivo continua o anterior até o reinício."
        }
        "admin.flags.col.flag" => "Flag",
        "admin.flags.col.effective" => "Efetiva agora",
        "admin.flags.col.stored" => "Gravada",
        "admin.flags.col.status" => "Situação",
        "admin.flags.col.action" => "Ação",
        "admin.flag.on" => "Ligada",
        "admin.flag.off" => "Desligada",
        "admin.flag.none" => "Nenhuma",
        "admin.flag.enable" => "Ligar",
        "admin.flag.disable" => "Desligar",
        "admin.flag.pending" => "Pendente de reinício",
        "admin.flag.not_implemented" => "Não implementado",
        "admin.stats.title" => "Saúde da instância",
        "admin.stats.users" => "Contas por situação",
        "admin.stats.db_size" => "Tamanho do banco (arquivo + WAL)",
        "admin.stats.migrations" => "Migrations aplicadas",
        "admin.stats.catalog" => "Catálogo territorial",
        "admin.stats.effective" => "Flags efetivas",
        _ => return None,
    })
}
