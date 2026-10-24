pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.audit" => "Auditoria",
        "audit.title" => "Auditoria e transparência",
        "audit.lead" => {
            "Registro público de eventos, sem e-mail, biometria, IP ou vínculo com cédulas."
        }
        "audit.chain.title" => "Integridade da cadeia local",
        "audit.chain.valid" => "Cadeia local verificada pelo servidor: sem quebras.",
        "audit.chain.broken" => "Alerta de integridade: cadeia quebrada no evento",
        "audit.chain.events" => "Eventos na cadeia (públicos e restritos)",
        "audit.chain.head" => "Hash do último evento",
        "audit.chain.note" => {
            "A verificação recalcula os hashes desta cópia. Ela não prova resistência a um operador com acesso ao servidor; para isso, compare com checkpoints assinados guardados fora da VPS."
        }
        "audit.events.title" => "Eventos",
        "audit.events.empty" => "Nenhum evento registrado.",
        "audit.filter.kind" => "Tipo de evento",
        "audit.filter.all" => "Todos",
        "audit.filter.apply" => "Filtrar",
        "audit.col.seq" => "Nº",
        "audit.col.time" => "Data (UTC)",
        "audit.col.kind" => "Tipo",
        "audit.col.subject" => "Objeto",
        "audit.col.payload" => "Dados",
        "audit.col.actor" => "Ator",
        "audit.col.hash" => "Hash",
        "audit.visibility.restricted" => "restrito",
        "audit.older" => "Eventos mais antigos",
        "audit.newest" => "Mais recentes",
        "audit.checkpoints.title" => "Checkpoints assinados",
        "audit.checkpoints.none" => "Nenhum checkpoint exportado ainda.",
        "audit.checkpoints.note" => {
            "Checkpoints (Ed25519) só detectam reescrita do histórico se a cópia exportada estiver guardada fora da VPS e a chave pública for conferida por outro canal."
        }
        "audit.public_key" => "Chave pública dos checkpoints",
        "audit.public_key.none" => "não configurada",
        "audit.advanced.title" => "Auditoria avançada",
        "audit.advanced.lead" => {
            "Inclui eventos restritos. Cada acesso é registrado na própria cadeia de auditoria."
        }
        "audit.advanced.link" => "Auditoria avançada (auditores)",
        "audit.back_public" => "Voltar à auditoria pública",
        _ => return None,
    })
}
