//! Identity: registration, enrollment, login, recovery, reauthentication.

pub fn lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.login" => "Entrar",
        "nav.register" => "Cadastrar",
        "nav.profile" => "Perfil",
        "nav.logout" => "Sair",
        "nav.finish" => "Concluir cadastro",
        "common.too_long" => "Valor longo demais.",

        "auth.field.name" => "Nome",
        "auth.field.email" => "E-mail",
        "auth.field.password" => "Senha",
        "auth.field.current_password" => "Senha atual",
        "auth.field.new_password" => "Nova senha",
        "auth.field.cep" => "CEP",
        "auth.field.totp" => "Código do autenticador (6 dígitos)",
        "auth.field.recovery_code" => "Código de recuperação",
        "auth.name.invalid" => "Informe seu nome (2 a 120 caracteres).",
        "auth.email.invalid" => "Informe um e-mail válido.",
        "auth.password.policy" => "A senha deve ter de 12 a 128 caracteres.",

        "auth.cep.lookup" => "Buscar endereço",
        "auth.cep.looking" => "Buscando CEP…",
        "auth.cep.invalid" => "CEP inválido: informe 8 dígitos.",
        "auth.cep.not_found" => "CEP não encontrado.",
        "auth.cep.unavailable" => {
            "Serviço de CEP indisponível ou sem resposta. Tente novamente em instantes."
        }
        "auth.cep.invalid_response" => {
            "O serviço de CEP devolveu uma resposta inválida. Tente novamente mais tarde."
        }
        "auth.cep.disabled" => "Busca de endereço indisponível nesta instância.",
        "auth.cep.unresolved" => {
            "Não foi possível identificar o município deste CEP. O cadastro não pode continuar com este endereço."
        }

        "auth.register.title" => "Criar conta",
        "auth.register.submit" => "Continuar",
        "auth.register.address_locked" => {
            "Confira o endereço: após o cadastro ele não poderá ser alterado nesta versão."
        }
        "auth.register.have_account" => "Já tenho conta",
        "auth.register.signed_in" => "Você já está conectado. Saia para criar outra conta.",
        "auth.register.unavailable" => {
            "Não foi possível concluir o cadastro com estes dados. Se você já tem conta, entre ou recupere a senha."
        }

        "auth.enroll.title" => "Validação de identidade",
        "auth.enroll.face" => "1. Rosto",
        "auth.enroll.face_hint" => {
            "Registre seu rosto. A imagem não é guardada; só uma referência protegida."
        }
        "auth.enroll.totp" => "2. Autenticador (TOTP)",
        "auth.enroll.totp_hint" => {
            "Adicione esta conta a um aplicativo autenticador e informe o código de 6 dígitos."
        }
        "auth.enroll.totp_secret" => "Chave:",
        "auth.enroll.totp_link" => "Abrir no aplicativo autenticador",
        "auth.enroll.totp_confirm" => "Confirmar código",
        "auth.enroll.activated" => "Conta ativada.",
        "auth.enroll.continue" => "Guardei os códigos, continuar",
        "auth.enroll.complete" => "Validação concluída.",
        "auth.enroll.reenrolled" => "Autenticador recadastrado.",
        "auth.enroll.not_allowed" => "Esta etapa não está disponível para a sua conta agora.",
        "auth.codes.title" => "Códigos de recuperação",
        "auth.codes.hint" => {
            "Guarde estes códigos em local seguro. Cada um substitui o autenticador uma única vez e eles não serão mostrados de novo."
        }

        "auth.login.title" => "Entrar",
        "auth.login.submit" => "Continuar",
        "auth.login.forgot" => "Esqueci minha senha",
        "auth.login.face" => "Verificação facial",
        "auth.login.totp" => "Código do autenticador",
        "auth.login.recovery" => "Código de recuperação",
        "auth.login.lost_totp" => "Perdi meu autenticador",
        "auth.login.use_totp" => "Usar o autenticador",

        "auth.reset.title" => "Recuperar senha",
        "auth.reset.hint" => {
            "Informe seu e-mail e confirme rosto e código do autenticador. Nenhum e-mail é enviado."
        }
        "auth.reset.continue" => "Continuar",
        "auth.reset.submit" => "Definir nova senha",
        "auth.reset.done" => {
            "Senha redefinida. Todas as sessões foram encerradas; entre novamente."
        }

        "auth.error.invalid" => "E-mail ou senha inválidos.",
        "auth.error.face" => "Verificação facial não confirmada. Tente novamente.",
        "auth.error.face_challenge" => "Captura expirada ou já usada. Capture o rosto novamente.",
        "auth.error.face_image" => "Imagem do rosto inválida.",
        "auth.error.totp" => "Código inválido ou já usado. Aguarde o próximo código.",
        "auth.error.recovery_code" => "Código de recuperação inválido ou já usado.",
        "auth.error.flow_expired" => "Etapa expirada. Recomece.",
        "auth.error.reenroll_totp" => {
            "Recadastre seu autenticador antes de ações críticas (Perfil → recadastrar)."
        }
        "auth.reauth.invalid_action" => "Ação inválida.",

        "face.synthetic.legend" => "Rosto sintético (teste)",
        "face.synthetic.hint" => {
            "Ambiente de teste: a frase substitui a câmera e não tem validade biométrica."
        }
        "face.synthetic.phrase" => "Frase do rosto sintético",
        "face.synthetic.use" => "Usar rosto sintético",
        "face.camera.hint" => "Posicione o rosto, com boa iluminação, e capture.",
        "face.camera.preview" => "Pré-visualização da câmera",
        "face.camera.start" => "Ativar câmera",
        "face.camera.capture" => "Capturar rosto",
        "face.camera.denied" => {
            "Permissão de câmera negada. Libere a câmera nas configurações do navegador para continuar."
        }
        "face.camera.unsupported" => "Câmera indisponível ou não compatível neste dispositivo.",
        "face.camera.timeout" => "A câmera não respondeu a tempo. Tente novamente.",
        "face.camera.failed" => "Não foi possível capturar a imagem. Tente novamente.",
        "face.captured" => "Rosto capturado.",
        "face.required" => "Capture o rosto antes de continuar.",

        "reauth.title" => "Confirme sua identidade",
        "reauth.hint" => "Esta ação exige rosto e código do autenticador.",
        "reauth.submit" => "Confirmar",

        "account.password.title" => "Alterar senha",
        "account.password.hint" => {
            "Exige senha atual, rosto e código. Todas as sessões serão encerradas."
        }
        "account.password.submit" => "Alterar senha",
        "account.codes.title" => "Novos códigos de recuperação",
        "account.codes.hint" => "Gera dez novos códigos e invalida os anteriores.",
        "account.codes.submit" => "Gerar novos códigos",
        _ => return None,
    })
}
