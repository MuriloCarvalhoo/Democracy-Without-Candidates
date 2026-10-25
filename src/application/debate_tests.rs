//! Debate rules against the real TursoDB engine: territory, phases, locks, edit
//! history, reports, moderation/audit, rate limits, drafts and amendments.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::America::Sao_Paulo;

use super::{
    actor::{self, Actor},
    audit::{self, ChainStatus, Scope},
    debate,
    proposals::Ctx,
    session::{SessionPolicy, testing::account_with_session, token_hash},
};
use crate::{
    domain::debate::{AmendmentChange, ModerationDecision, PostBlock, RATE_LIMIT, ReportReason},
    error::AppError,
    infrastructure::{
        catalog,
        clock::ts,
        crypto::new_id,
        db::{Db, query_all, query_opt, testing::TestDb, testing::migrated},
        flags::{DEBATE, Flags},
    },
};

const CATALOG: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n35,state,São Paulo,BR,SP\n3550308,municipal,São Paulo,35,SP\n";

struct World {
    t: TestDb,
    flags: Flags,
    now: DateTime<Utc>,
}

async fn jurisdiction(db: &Db, code: &str) -> String {
    let conn = db.read().await.unwrap();
    query_opt(
        &conn,
        "SELECT id FROM jurisdictions WHERE code = ?1",
        [code],
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
    .unwrap()
}

async fn world() -> World {
    let t = migrated().await;
    catalog::import_str(&t.db, "mini", CATALOG).await.unwrap();
    World {
        t,
        flags: Flags::with(&[DEBATE]),
        now: "2026-10-24T12:00:00Z".parse().unwrap(),
    }
}

impl World {
    fn ctx(&self) -> Ctx<'_> {
        self.at(self.now)
    }

    fn at(&self, now: DateTime<Utc>) -> Ctx<'_> {
        Ctx {
            db: &self.t.db,
            flags: &self.flags,
            now,
            clock: None,
            tz: Sao_Paulo,
        }
    }

    async fn account(&self, code: &str, status: &str, roles: &[&str]) -> Actor {
        let (user, token) = account_with_session(
            &self.t.db,
            self.now,
            SessionPolicy::from_secs(900, 28800),
            code,
            status,
            roles,
        )
        .await;
        let conn = self.t.db.read().await.unwrap();
        actor::load(&conn, &user, &token_hash(&token))
            .await
            .unwrap()
            .unwrap()
    }

    async fn citizen(&self, code: &str) -> Actor {
        self.account(code, "active", &["CITIZEN"]).await
    }

    async fn admin(&self) -> Actor {
        self.account("4202404", "active", &["COUNTRY_ADMIN"]).await
    }

    /// Proposal row inserted directly in `status` (published unless draft).
    async fn proposal(&self, code: &str, status: &str, author: &Actor) -> String {
        let id = new_id();
        let j = jurisdiction(&self.t.db, code).await;
        let published = (status != "draft").then(|| ts(self.now - Duration::days(1)));
        let hash = published.as_ref().map(|_| "a".repeat(64));
        let archived = (status == "archived").then(|| ts(self.now));
        let tx = self.t.db.write().await.unwrap();
        tx.execute(
            "INSERT INTO proposals (id, kind, jurisdiction_id, author_id, title, summary, body, normalized_title,
               normalized_body_hash, text_hash, status, created_at, updated_at, published_at, archived_at)
             VALUES (?1, 'normal_law', ?2, ?3, 'Título de teste', 'Resumo de teste', 'Texto de teste publicado', '', '', ?4, ?5, ?6, ?6, ?7, ?8)",
            (id.as_str(), j, author.user_id.as_str(), hash, status, ts(self.now), published, archived),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    async fn sql(&self, sql: &str, params: impl turso::IntoParams) {
        let tx = self.t.db.write().await.unwrap();
        tx.execute(sql, params).await.unwrap();
        tx.commit().await.unwrap();
    }

    async fn scalar(&self, sql: &str, params: impl turso::IntoParams) -> i64 {
        let conn = self.t.db.read().await.unwrap();
        query_opt(&conn, sql, params)
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }

    async fn view(
        &self,
        viewer: Option<&Actor>,
        id: &str,
    ) -> crate::domain::debate::view::DebateView {
        let conn = self.t.db.read().await.unwrap();
        debate::thread(&conn, &self.flags, self.now, Sao_Paulo, viewer, id, 1)
            .await
            .unwrap()
    }

    async fn post(
        &self,
        a: &Actor,
        id: &str,
        parent: Option<&str>,
        body: &str,
    ) -> Result<String, AppError> {
        debate::post_comment(&self.ctx(), a, id, parent, body).await
    }
}

#[tokio::test]
async fn territory_and_account_status_gate_posting() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let municipal = w.proposal("4202404", "in_debate", &author).await;
    let national = w.proposal("BR", "in_debate", &author).await;
    let neighbor = w.citizen("4202404").await;
    let other_city = w.citizen("4205407").await;
    let other_state = w.citizen("3550308").await;
    let pending = w.account("4202404", "pending", &["CITIZEN"]).await;
    let admin = w.admin().await;

    w.post(&neighbor, &municipal, None, "Concordo com a proposta.")
        .await
        .unwrap();
    assert_eq!(
        w.post(&other_city, &municipal, None, "Outro município")
            .await,
        Err(AppError::Forbidden)
    );
    w.post(&other_state, &national, None, "Nacional vale para todos")
        .await
        .unwrap();
    assert_eq!(
        w.post(&pending, &municipal, None, "Pendente").await,
        Err(AppError::Forbidden)
    );
    assert_eq!(
        w.post(&admin, &municipal, None, "Admin sem papel de cidadão")
            .await,
        Err(AppError::Forbidden),
        "moderation role alone does not post"
    );

    let v = w.view(Some(&other_city), &municipal).await;
    assert_eq!(v.viewer.unwrap().block, Some(PostBlock::NotEligible));
    assert!(!v.comments[0].can_reply);
    let v = w.view(None, &municipal).await;
    assert!(v.viewer.is_none(), "visitors read without posting");
    assert_eq!(v.comments.len(), 1);
    assert!(!v.comments[0].can_report);
}

#[tokio::test]
async fn phases_decide_comments_and_amendments() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    for status in ["collecting_signatures", "in_debate", "in_voting"] {
        let id = w.proposal("4202404", status, &author).await;
        w.post(&c, &id, None, "Comentário na fase").await.unwrap();
        let amendment = debate::propose_amendment(
            &w.ctx(),
            &c,
            &id,
            "Emenda de teste",
            "Texto sugerido com mais de vinte caracteres",
        )
        .await;
        if status == "in_voting" {
            assert_eq!(
                amendment,
                Err(AppError::PhaseClosed),
                "no amendments during voting"
            );
        } else {
            amendment.unwrap();
        }
    }
    for status in ["approved", "rejected", "expired", "archived"] {
        let id = w.proposal("4202404", status, &author).await;
        assert_eq!(
            w.post(&c, &id, None, "Tarde demais").await,
            Err(AppError::PhaseClosed)
        );
        // Closed debates stay publicly readable.
        assert_eq!(w.view(None, &id).await.status.as_db(), status);
    }
    // Deadline reached before the ticker moved the phase: closed as well.
    let id = w.proposal("4202404", "in_debate", &author).await;
    w.sql(
        "UPDATE proposals SET debate_ends_at = ?2 WHERE id = ?1",
        (id.as_str(), ts(w.now - Duration::seconds(1))),
    )
    .await;
    assert_eq!(
        w.post(&c, &id, None, "Prazo encerrado").await,
        Err(AppError::PhaseClosed)
    );
}

#[tokio::test]
async fn drafts_are_not_readable_and_the_flag_gates_reads() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let draft = w.proposal("4202404", "draft", &author).await;
    let conn = w.t.db.read().await.unwrap();
    for viewer in [None, Some(&author)] {
        assert_eq!(
            debate::thread(&conn, &w.flags, w.now, Sao_Paulo, viewer, &draft, 1)
                .await
                .map(|_| ()),
            Err(AppError::NotFound)
        );
        assert_eq!(
            debate::amendments(&conn, &w.flags, w.now, Sao_Paulo, viewer, &draft)
                .await
                .map(|_| ()),
            Err(AppError::NotFound)
        );
    }
    assert_eq!(
        w.post(&author, &draft, None, "Rascunho").await,
        Err(AppError::NotFound)
    );
    let flags_off = Flags::default();
    let published = w.proposal("4202404", "in_debate", &author).await;
    assert!(matches!(
        debate::thread(&conn, &flags_off, w.now, Sao_Paulo, None, &published, 1).await,
        Err(AppError::FeatureDisabled(_))
    ));
}

#[tokio::test]
async fn lock_needs_reason_blocks_posts_and_is_recorded() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let admin = w.admin().await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let comment = w.post(&c, &id, None, "Antes do bloqueio").await.unwrap();

    assert_eq!(
        debate::set_lock(&w.ctx(), &c, &id, true, "Motivo qualquer").await,
        Err(AppError::Forbidden)
    );
    assert!(matches!(
        debate::set_lock(&w.ctx(), &admin, &id, true, "  ").await,
        Err(AppError::Validation { .. })
    ));
    debate::set_lock(&w.ctx(), &admin, &id, true, "Ataques pessoais repetidos")
        .await
        .unwrap();
    assert!(matches!(
        debate::set_lock(&w.ctx(), &admin, &id, true, "De novo, mesmo motivo").await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        w.post(&c, &id, None, "Bloqueado").await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        debate::edit_comment(&w.ctx(), &c, &comment, "Edição bloqueada").await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        debate::propose_amendment(
            &w.ctx(),
            &c,
            &id,
            "Emenda bloqueada",
            "Texto sugerido com mais de vinte caracteres"
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    let v = w.view(Some(&c), &id).await;
    assert_eq!(v.lock.unwrap().reason, "Ataques pessoais repetidos");
    assert_eq!(v.viewer.unwrap().block, Some(PostBlock::Locked));
    assert_eq!(
        v.status.as_db(),
        "in_debate",
        "lock never changes the phase"
    );

    debate::set_lock(&w.ctx(), &admin, &id, false, "Debate reaberto após revisão")
        .await
        .unwrap();
    w.post(&c, &id, None, "Depois do desbloqueio")
        .await
        .unwrap();
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM moderation_actions WHERE target_kind = 'thread' AND target_id = ?1",
            [id.as_str()]
        )
        .await,
        2
    );
}

#[tokio::test]
async fn edits_keep_history_and_show_edited() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let other = w.citizen("4202404").await;
    let admin = w.admin().await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let comment = w.post(&c, &id, None, "Versão original").await.unwrap();
    assert!(w.view(None, &id).await.comments[0].edited_at.is_none());

    debate::edit_comment(&w.ctx(), &c, &comment, "Versão corrigida")
        .await
        .unwrap();
    debate::edit_comment(&w.ctx(), &c, &comment, "Versão final\ncom quebra de linha")
        .await
        .unwrap();
    assert_eq!(
        debate::edit_comment(&w.ctx(), &other, &comment, "Não é meu").await,
        Err(AppError::Forbidden)
    );
    let v = w.view(None, &id).await;
    assert!(v.comments[0].edited_at.is_some());
    assert_eq!(
        v.comments[0].body.as_deref(),
        Some("Versão final\ncom quebra de linha")
    );

    let conn = w.t.db.read().await.unwrap();
    let revs = debate::history(&conn, &w.flags, Sao_Paulo, &c, &comment)
        .await
        .unwrap();
    assert_eq!(
        revs.iter().map(|r| r.body.as_str()).collect::<Vec<_>>(),
        [
            "Versão original",
            "Versão corrigida",
            "Versão final\ncom quebra de linha"
        ]
    );
    assert_eq!(
        debate::history(&conn, &w.flags, Sao_Paulo, &admin, &comment)
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        debate::history(&conn, &w.flags, Sao_Paulo, &other, &comment).await,
        Err(AppError::Forbidden)
    );
    assert!(
        conn.execute("UPDATE comment_revisions SET body = 'x'", ())
            .await
            .is_err()
    );
    assert!(
        conn.execute("DELETE FROM comment_revisions", ())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn one_report_per_account_and_queue_is_restricted() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let r1 = w.citizen("4202404").await;
    let r2 = w.citizen("4205407").await;
    let admin = w.admin().await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let comment = w.post(&c, &id, None, "Comentário polêmico").await.unwrap();

    debate::report_comment(&w.ctx(), &r1, &comment, ReportReason::Offensive, "")
        .await
        .unwrap();
    assert!(matches!(
        debate::report_comment(&w.ctx(), &r1, &comment, ReportReason::Spam, "").await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        debate::report_comment(&w.ctx(), &c, &comment, ReportReason::Spam, "").await,
        Err(AppError::Forbidden),
        "own comment"
    );
    assert!(matches!(
        debate::report_comment(&w.ctx(), &r2, &comment, ReportReason::Other, " ").await,
        Err(AppError::Validation { .. })
    ));
    // Any active citizen may report, also outside the jurisdiction.
    debate::report_comment(
        &w.ctx(),
        &r2,
        &comment,
        ReportReason::Other,
        "Contém dados pessoais",
    )
    .await
    .unwrap();

    let v = w.view(Some(&r1), &id).await;
    assert!(v.comments[0].reported && !v.comments[0].can_report);
    assert_eq!(
        v.comments[0].open_reports, None,
        "report counts are for moderators"
    );
    assert_eq!(
        w.view(Some(&admin), &id).await.comments[0].open_reports,
        Some(2)
    );

    let conn = w.t.db.read().await.unwrap();
    assert_eq!(
        debate::moderation_queue(&conn, &w.flags, Sao_Paulo, &r1).await,
        Err(AppError::Forbidden)
    );
    let queue = debate::moderation_queue(&conn, &w.flags, Sao_Paulo, &admin)
        .await
        .unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].reasons.len(), 2);
    assert_eq!(queue[0].details, ["Contém dados pessoais"]);

    debate::moderate_comment(
        &w.ctx(),
        &admin,
        &comment,
        ModerationDecision::Dismiss,
        "Crítica legítima ao texto",
    )
    .await
    .unwrap();
    assert!(
        debate::moderation_queue(&conn, &w.flags, Sao_Paulo, &admin)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        debate::moderate_comment(
            &w.ctx(),
            &admin,
            &comment,
            ModerationDecision::Dismiss,
            "Nada a arquivar"
        )
        .await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn moderation_hides_publicly_keeps_content_and_audits() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let reporter = w.citizen("4202404").await;
    let admin = w.admin().await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let comment = w
        .post(&c, &id, None, "Conteúdo abusivo sintético")
        .await
        .unwrap();
    debate::report_comment(&w.ctx(), &reporter, &comment, ReportReason::Offensive, "")
        .await
        .unwrap();

    assert_eq!(
        debate::moderate_comment(
            &w.ctx(),
            &reporter,
            &comment,
            ModerationDecision::Remove,
            "Não sou moderador"
        )
        .await,
        Err(AppError::Forbidden)
    );
    assert!(matches!(
        debate::moderate_comment(&w.ctx(), &admin, &comment, ModerationDecision::Remove, "ok")
            .await,
        Err(AppError::Validation { .. })
    ));
    debate::moderate_comment(
        &w.ctx(),
        &admin,
        &comment,
        ModerationDecision::Remove,
        "Ofensa a outra pessoa",
    )
    .await
    .unwrap();

    let public = w.view(None, &id).await;
    assert!(public.comments[0].removed);
    assert_eq!(
        public.comments[0].body, None,
        "content hidden from the public view"
    );
    assert_eq!(
        w.view(Some(&admin), &id).await.comments[0].body.as_deref(),
        Some("Conteúdo abusivo sintético")
    );
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM comments WHERE id = ?1 AND body = 'Conteúdo abusivo sintético'",
            [comment.as_str()]
        )
        .await,
        1,
        "content kept for audit"
    );
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM comment_reports WHERE comment_id = ?1 AND status = 'actioned'",
            [comment.as_str()]
        )
        .await,
        1
    );
    assert!(matches!(
        w.post(&reporter, &id, Some(&comment), "Resposta a removido")
            .await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        debate::edit_comment(&w.ctx(), &c, &comment, "Tentativa de edição").await,
        Err(AppError::Conflict(_))
    ));

    // Audit: public event without content or moderator; restricted with both.
    let conn = w.t.db.read().await.unwrap();
    let (public_events, _) =
        audit::list(&conn, Scope::Public, Some("debate.comment_removed"), None)
            .await
            .unwrap();
    assert_eq!(public_events.len(), 1);
    assert_eq!(public_events[0].subject.as_deref(), Some(comment.as_str()));
    assert!(public_events[0].actor_ref.is_none());
    assert!(
        !public_events[0].payload.contains("abusivo")
            && !public_events[0].payload.contains("Ofensa")
    );
    let (restricted, _) = audit::list(&conn, Scope::Restricted, Some("debate.moderation"), None)
        .await
        .unwrap();
    assert_eq!(restricted.len(), 1);
    assert_eq!(
        restricted[0].actor_ref.as_deref(),
        Some(admin.user_id.as_str())
    );
    assert!(restricted[0].payload.contains("Ofensa a outra pessoa"));
    assert!(
        audit::list(&conn, Scope::Public, Some("debate.moderation"), None)
            .await
            .unwrap()
            .0
            .is_empty()
    );
    assert!(matches!(
        audit::verify(&conn).await.unwrap(),
        ChainStatus::Valid { .. }
    ));
    // The author is told why (always delivered).
    let notes = query_all(
        &conn,
        "SELECT title, body FROM notifications WHERE user_id = ?1 AND event = 'debate.moderation'",
        [c.user_id.as_str()],
    )
    .await
    .unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].get::<String>(1).unwrap(), "Ofensa a outra pessoa");

    debate::moderate_comment(
        &w.ctx(),
        &admin,
        &comment,
        ModerationDecision::Restore,
        "Revisão: contexto de citação",
    )
    .await
    .unwrap();
    assert_eq!(
        w.view(None, &id).await.comments[0].body.as_deref(),
        Some("Conteúdo abusivo sintético")
    );
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM moderation_actions WHERE target_id = ?1",
            [comment.as_str()]
        )
        .await,
        2
    );
    assert!(conn.execute("DELETE FROM comments", ()).await.is_err());
    assert!(
        conn.execute("UPDATE moderation_actions SET reason = 'x'", ())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn posting_is_rate_limited_with_persisted_counts() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let first = w.post(&c, &id, None, "Comentário 0").await.unwrap();
    for n in 1..RATE_LIMIT - 1 {
        w.post(&c, &id, None, &format!("Comentário {n}"))
            .await
            .unwrap();
    }
    debate::edit_comment(&w.ctx(), &c, &first, "Edições contam no limite")
        .await
        .unwrap();
    assert_eq!(
        w.post(&c, &id, None, "Excesso").await,
        Err(AppError::RateLimited)
    );
    assert_eq!(
        debate::edit_comment(&w.ctx(), &c, &first, "Outra edição").await,
        Err(AppError::RateLimited)
    );
    // Another account is not affected; the window slides.
    w.post(&author, &id, None, "Autor da proposta comenta")
        .await
        .unwrap();
    let later = w.at(w.now + Duration::minutes(11));
    debate::post_comment(&later, &c, &id, None, "Depois da janela")
        .await
        .unwrap();
    assert!(matches!(
        w.post(&c, &id, None, &"x".repeat(2001)).await,
        Err(AppError::Validation { .. })
    ));
    assert!(matches!(
        w.post(&c, &id, None, "   ").await,
        Err(AppError::Validation { .. })
    ));
}

#[tokio::test]
async fn threads_are_bounded_and_ordered_depth_first() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let a = w.post(&c, &id, None, "A").await.unwrap();
    let b = w.post(&c, &id, None, "B").await.unwrap();
    let a1 = w.post(&author, &id, Some(&a), "A1").await.unwrap();
    let a1x = w.post(&c, &id, Some(&a1), "A1x").await.unwrap();
    assert!(matches!(
        w.post(&author, &id, Some(&a1x), "Fundo demais").await,
        Err(AppError::Validation { .. })
    ));
    let other = w.proposal("4202404", "in_debate", &author).await;
    assert_eq!(
        w.post(&c, &other, Some(&a), "Outra proposta").await,
        Err(AppError::NotFound)
    );
    let v = w.view(Some(&c), &id).await;
    assert_eq!(
        v.comments
            .iter()
            .map(|c| (c.body.clone().unwrap(), c.depth))
            .collect::<Vec<_>>(),
        [
            ("A".into(), 0),
            ("A1".into(), 1),
            ("A1x".into(), 2),
            ("B".into(), 0)
        ]
    );
    assert!(!v.comments[2].can_reply, "max depth");
    assert_eq!(v.threads, 2);
    let _ = b;
}

#[tokio::test]
async fn proposal_author_is_notified_once_per_unread_activity() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    let first = w.post(&c, &id, None, "Primeiro").await.unwrap();
    w.post(&c, &id, None, "Segundo").await.unwrap();
    w.post(&c, &id, Some(&first), "Resposta").await.unwrap();
    w.post(&author, &id, None, "Comentário do próprio autor")
        .await
        .unwrap();
    let count = |link: String| {
        let w = &w;
        let user = author.user_id.clone();
        async move {
            w.scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'debate.activity' AND link = ?2",
                (user, link),
            )
            .await
        }
    };
    assert_eq!(count(format!("/propostas/{id}/debate")).await, 1);
    debate::propose_amendment(
        &w.ctx(),
        &c,
        &id,
        "Emenda notificada",
        "Texto sugerido com mais de vinte caracteres",
    )
    .await
    .unwrap();
    assert_eq!(count(format!("/propostas/{id}/debate/emenda")).await, 1);
}

#[tokio::test]
async fn amendments_never_change_the_published_text() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let c = w.citizen("4202404").await;
    let other = w.citizen("4202404").await;
    let id = w
        .proposal("4202404", "collecting_signatures", &author)
        .await;
    let a = debate::propose_amendment(
        &w.ctx(),
        &c,
        &id,
        "Incluir ciclofaixa",
        "Art. 1º Incluir ciclofaixa protegida na via.",
    )
    .await
    .unwrap();
    let b = debate::propose_amendment(
        &w.ctx(),
        &other,
        &id,
        "Outra sugestão",
        "Art. 2º Prazo de implantação de 180 dias.",
    )
    .await
    .unwrap();
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM proposals WHERE id = ?1 AND body = 'Texto de teste publicado'",
            [id.as_str()]
        )
        .await,
        1
    );
    let conn = w.t.db.read().await.unwrap();
    let list = debate::amendments(&conn, &w.flags, w.now, Sao_Paulo, Some(&author), &id)
        .await
        .unwrap();
    assert_eq!(list.items.len(), 2);
    assert!(
        list.items
            .iter()
            .all(|i| i.current_text && i.can_address && i.text_hash == list.text_hash)
    );

    assert_eq!(
        debate::change_amendment(&w.ctx(), &other, &a, AmendmentChange::Withdraw, "").await,
        Err(AppError::Forbidden)
    );
    assert_eq!(
        debate::change_amendment(&w.ctx(), &c, &a, AmendmentChange::Address, "Será incluída").await,
        Err(AppError::Forbidden),
        "only the proposal author addresses"
    );
    debate::change_amendment(
        &w.ctx(),
        &author,
        &a,
        AmendmentChange::Address,
        "Vai virar nova proposta",
    )
    .await
    .unwrap();
    debate::change_amendment(&w.ctx(), &other, &b, AmendmentChange::Withdraw, "")
        .await
        .unwrap();
    assert!(matches!(
        debate::change_amendment(&w.ctx(), &other, &b, AmendmentChange::Withdraw, "").await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        w.scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'debate.activity'",
            [c.user_id.as_str()]
        )
        .await,
        1,
        "amendment author told it was addressed"
    );
    assert!(
        conn.execute("UPDATE amendments SET body = 'x'", ())
            .await
            .is_err()
    );
    assert!(
        conn.execute("UPDATE amendments SET status = 'open'", ())
            .await
            .is_err()
    );
    assert!(conn.execute("DELETE FROM amendments", ()).await.is_err());
    // Daily limit per account.
    for n in 1..5 {
        debate::propose_amendment(
            &w.ctx(),
            &c,
            &id,
            &format!("Sugestão {n:02}"),
            "Texto sugerido com mais de vinte caracteres",
        )
        .await
        .unwrap();
    }
    assert_eq!(
        debate::propose_amendment(
            &w.ctx(),
            &c,
            &id,
            "Sexta sugestão",
            "Texto sugerido com mais de vinte caracteres"
        )
        .await,
        Err(AppError::RateLimited)
    );
}

#[tokio::test]
async fn author_labels_respect_profile_visibility() {
    let w = world().await;
    let author = w.citizen("4202404").await;
    let private = w.citizen("4202404").await;
    let public = w.citizen("4202404").await;
    w.sql(
        "INSERT INTO user_preferences (user_id, profile_visibility, updated_at) VALUES (?1, 'public', 'now')",
        [public.user_id.as_str()],
    )
    .await;
    let id = w.proposal("4202404", "in_debate", &author).await;
    w.post(&private, &id, None, "Perfil privado").await.unwrap();
    w.post(&public, &id, None, "Perfil público").await.unwrap();
    let v = w.view(Some(&private), &id).await;
    assert_eq!(v.comments[0].author, "Cidadão(ã)");
    assert!(v.comments[0].own && v.comments[0].can_edit);
    assert_eq!(v.comments[1].author, "Cidadã Sintética");
    assert!(!v.comments[1].own && v.comments[1].can_report);
}
