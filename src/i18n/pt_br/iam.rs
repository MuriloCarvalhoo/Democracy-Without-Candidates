pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "iam.nav" => "Seções de permissões",
        "iam.nav.policies" => "Políticas",
        "iam.nav.groups" => "Grupos",
        "iam.nav.users" => "Usuários",
        "iam.nav.simulator" => "Simulador",
        "iam.policies.title" => "Políticas de acesso",
        "iam.policies.create" => "Criar política",
        "iam.policies.hint" => {
            "Documento JSON no formato IAM: Effect Allow/Deny, Action (catálogo fixo, aceita * e serviço:*) e Resource (* ou jurisdiction:<código IBGE>, com * no fim). Deny explícito vence qualquer Allow. Versão aceita:"
        }
        "iam.policies.attachments" => "Anexos",
        "iam.policies.version" => "Versão",
        "iam.policy.title" => "Política",
        "iam.policy.managed_badge" => "Gerenciada",
        "iam.policy.managed_hint" => {
            "Política gerenciada pela instância: não pode ser editada nem excluída."
        }
        "iam.policy.managed" => "Políticas gerenciadas não podem ser alteradas nem excluídas.",
        "iam.policy.attached" => {
            "A política ainda está anexada a usuários ou grupos. Desanexe antes de excluir."
        }
        "iam.groups.title" => "Grupos",
        "iam.groups.create" => "Criar grupo",
        "iam.groups.empty" => "Nenhum grupo criado.",
        "iam.groups.members" => "Membros",
        "iam.groups.add_member" => "Adicionar membro (e-mail)",
        "iam.group.title" => "Grupo",
        "iam.users.title" => "Usuários",
        "iam.users.query" => "E-mail (início)",
        "iam.users.search" => "Buscar",
        "iam.user.title" => "Permissões do usuário",
        "iam.user.no_groups" => "Sem grupos.",
        "iam.user.effective" => "Permissões efetivas",
        "iam.user.effective_hint" => {
            "Ações permitidas em todos os recursos, já aplicando situação da conta, Publico, políticas diretas e grupos."
        }
        "iam.user.not_found" => "Conta não encontrada.",
        "iam.user.query_invalid" => "Informe ao menos 2 caracteres do e-mail.",
        "iam.field.name" => "Nome",
        "iam.field.description" => "Descrição",
        "iam.field.document" => "Documento (JSON)",
        "iam.name.invalid" => "Nome inválido: use 2 a 64 letras, números, - ou _.",
        "iam.name.taken" => "Já existe um item com este nome.",
        "iam.description.invalid" => "Descrição inválida (até 300 caracteres).",
        "iam.already_attached" => "Já está anexado.",
        "iam.lockout" => {
            "Mudança recusada: a instância ficaria sem nenhum administrador com AdministratorAccess capaz de gerenciar permissões."
        }
        "iam.attach.policy" => "Política",
        "iam.attach" => "Anexar",
        "iam.detach" => "Desanexar",
        "iam.add" => "Adicionar",
        "iam.remove" => "Remover",
        "iam.save" => "Salvar",
        "iam.delete" => "Excluir",
        "iam.cancel" => "Cancelar",
        "iam.done" => "Alteração aplicada e auditada.",
        "iam.reauth_hint" => {
            "Mudanças de permissão exigem confirmar rosto e TOTP (uma confirmação por mudança)."
        }
        "iam.catalog.title" => "Catálogo de ações",
        "iam.catalog.hint" => {
            "Ações fora do catálogo são recusadas. Situação da conta, território, autoria, fase e reautenticação continuam valendo mesmo com Allow."
        }
        "iam.sim.title" => "Simulador de políticas",
        "iam.sim.hint" => {
            "Avalia uma ação como numa requisição real da conta, mostrando a declaração que decidiu."
        }
        "iam.sim.email" => "Conta (e-mail)",
        "iam.sim.action" => "Ação",
        "iam.sim.resource" => "Recurso (* ou jurisdiction:<código>)",
        "iam.sim.submit" => "Simular",
        "iam.sim.request" => "Requisição",
        "iam.sim.decision" => "Decisão",
        "iam.sim.reason" => "Motivo",
        "iam.sim.allowed" => "Permitido",
        "iam.sim.denied" => "Negado",
        "iam.sim.implicit" => "Negação implícita: nenhuma declaração corresponde.",
        "iam.sim.statement" => "Declaração da política",
        "iam.doc.too_large" => "Documento grande demais (máximo 8 KiB).",
        "iam.doc.json" => "JSON inválido ou campo não suportado:",
        "iam.doc.version" => "Version não suportada; use",
        "iam.doc.statements" => "Statement deve ter de 1 a 32 declarações.",
        "iam.doc.entries" => "Action/Resource vazio ou longo demais na declaração",
        "iam.doc.sid" => "Sid inválido (apenas letras e números) na declaração",
        "iam.doc.unknown_action" => "Ação desconhecida:",
        "iam.doc.resource" => "Recurso inválido:",
        "iam.action.profile_read" => "Ver o próprio perfil",
        "iam.action.profile_update" => "Alterar o próprio perfil e privacidade",
        "iam.action.proposal_read" => "Ler propostas",
        "iam.action.proposal_create" => "Criar e publicar propostas",
        "iam.action.proposal_sign" => "Assinar propostas",
        "iam.action.proposal_vote" => "Votar em propostas",
        "iam.action.proposal_archive" => "Arquivar propostas encerradas",
        "iam.action.comment_create" => "Comentar, denunciar e propor emendas",
        "iam.action.setup_start" => "Iniciar o plebiscito de setup",
        "iam.action.setup_vote" => "Votar no setup",
        "iam.action.moderation_act" => "Moderar o debate",
        "iam.action.flags_manage" => "Alterar feature flags",
        "iam.action.admin_read" => "Ver o painel da instância",
        "iam.action.audit_read" => "Ler a auditoria pública",
        "iam.action.audit_read_restricted" => "Ler eventos restritos de auditoria",
        "iam.action.stats_read" => "Ver estatísticas",
        "iam.action.iam_read" => "Ver políticas, grupos e permissões",
        "iam.action.iam_manage" => "Gerenciar políticas, grupos e anexos",
        _ => return None,
    })
}
