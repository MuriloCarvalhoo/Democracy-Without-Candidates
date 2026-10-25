//! Legislative core against the real TursoDB engine: setup gate, drafts,
//! publication, quota, signatures, thresholds, deadlines and concurrency.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::America::Sao_Paulo;

use super::{
    actor::{self, Actor},
    phases, proposals,
    proposals::{Ctx, ListFilter},
    reauth,
    session::{
        SessionPolicy,
        testing::{attach_policy, citizen_with_session},
        token_hash,
    },
    setup, signatures,
};
use crate::{
    domain::{
        proposals::{Level, Outcome, ProposalKind, ProposalStatus, Tally, view::DraftInput},
        setup::{SUGGESTED, SetupParams},
    },
    error::AppError,
    infrastructure::{
        catalog,
        db::{
            Db, query_opt,
            testing::{TestDb, migrated},
        },
        flags::{Flags, PROPOSALS, SIGNATURES},
    },
};

const CATALOG: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n35,state,São Paulo,BR,SP\n3550308,municipal,São Paulo,35,SP\n";

struct World {
    t: TestDb,
    flags: Flags,
    br: String,
    bnu: String,
}

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
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
        br: jurisdiction(&t.db, "BR").await,
        bnu: jurisdiction(&t.db, "4202404").await,
        flags: Flags::with(&[PROPOSALS, SIGNATURES]),
        t,
    }
}

impl World {
    fn ctx(&self, now: DateTime<Utc>) -> Ctx<'_> {
        Ctx {
            db: &self.t.db,
            flags: &self.flags,
            now,
            clock: None,
            tz: Sao_Paulo,
        }
    }

    async fn citizen(&self, now: DateTime<Utc>, code: &str) -> Actor {
        let (user, token) =
            citizen_with_session(&self.t.db, now, SessionPolicy::from_secs(900, 28800), code).await;
        let conn = self.t.db.read().await.unwrap();
        actor::load(&conn, &user, &token_hash(&token))
            .await
            .unwrap()
            .unwrap()
    }

    async fn admin(&self, now: DateTime<Utc>) -> Actor {
        let a = self.citizen(now, "4202404").await;
        let tx = self.t.db.write().await.unwrap();
        attach_policy(&tx, &a.user_id, "AdministratorAccess").await;
        tx.commit().await.unwrap();
        let conn = self.t.db.read().await.unwrap();
        actor::load(&conn, &a.user_id, &a.session_hash)
            .await
            .unwrap()
            .unwrap()
    }

    async fn adopt(&self, now: DateTime<Utc>, params: &SetupParams) {
        let tx = self.t.db.write().await.unwrap();
        setup::adopt_setup(&tx, now, &self.br, params, None, true)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    async fn grant(&self, now: DateTime<Utc>, actor: &Actor, action: &str, id: &str) {
        let tx = self.t.db.write().await.unwrap();
        reauth::grant(&tx, now, &actor.session_hash, action, id)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    async fn draft(
        &self,
        now: DateTime<Utc>,
        author: &Actor,
        kind: ProposalKind,
        title: &str,
    ) -> String {
        proposals::save_draft(
            &self.ctx(now),
            author,
            None,
            &input(kind, Level::Municipal, title),
        )
        .await
        .unwrap()
        .id
    }

    async fn publish(
        &self,
        now: DateTime<Utc>,
        author: &Actor,
        kind: ProposalKind,
        title: &str,
    ) -> String {
        let id = self.draft(now, author, kind, title).await;
        self.grant(now, author, proposals::REAUTH_PUBLISH, &id)
            .await;
        proposals::publish(&self.ctx(now), author, &id, None)
            .await
            .unwrap();
        id
    }

    async fn sign(
        &self,
        now: DateTime<Utc>,
        who: &Actor,
        id: &str,
    ) -> Result<crate::domain::proposals::view::SignatureState, AppError> {
        self.grant(now, who, signatures::REAUTH_SIGN, id).await;
        signatures::sign(&self.ctx(now), who, id, None).await
    }

    async fn row(&self, id: &str) -> proposals::ProposalRow {
        proposals::load(&self.t.db.read().await.unwrap(), id)
            .await
            .unwrap()
            .unwrap()
    }

    async fn count(&self, sql: &str, param: &str) -> i64 {
        let conn = self.t.db.read().await.unwrap();
        query_opt(&conn, sql, [param])
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }
}

fn input(kind: ProposalKind, level: Level, title: &str) -> DraftInput {
    DraftInput {
        kind,
        level,
        title: title.into(),
        summary: "Resumo sintético da proposta".into(),
        body: format!("Texto completo sintético para {title}."),
        proposed_setup: (kind == ProposalKind::SetupChange).then_some(SUGGESTED),
    }
}

/// Signature target equals the whole municipal electorate.
fn all_must_sign() -> SetupParams {
    SetupParams {
        signature_bp_municipal: 10_000,
        ..SUGGESTED
    }
}

#[tokio::test]
async fn no_law_before_setup_and_drafts_are_private() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    let author = w.citizen(now, "4202404").await;
    let other = w.citizen(now, "4202404").await;
    let err = proposals::save_draft(
        &w.ctx(now),
        &author,
        None,
        &input(ProposalKind::NormalLaw, Level::Municipal, "Praça pública"),
    )
    .await;
    assert!(matches!(err, Err(AppError::Conflict(_))), "{err:?}");
    let initial = input(ProposalKind::InitialSetup, Level::National, "Plebiscito");
    assert_eq!(
        proposals::save_draft(&w.ctx(now), &author, None, &initial).await,
        Err(AppError::Forbidden)
    );

    w.adopt(now, &SUGGESTED).await;
    let id = w
        .draft(now, &author, ProposalKind::NormalLaw, "Praça pública")
        .await;
    let conn = w.t.db.read().await.unwrap();
    let r = w.row(&id).await;
    assert_eq!(
        (r.status, r.jurisdiction_id.as_str()),
        (ProposalStatus::Draft, w.bnu.as_str())
    );
    assert!(
        proposals::detail(&conn, &w.flags, now, Sao_Paulo, Some(&author), &id)
            .await
            .is_ok()
    );
    assert_eq!(
        proposals::detail(&conn, &w.flags, now, Sao_Paulo, Some(&other), &id)
            .await
            .err(),
        Some(AppError::NotFound)
    );
    assert_eq!(
        proposals::detail(&conn, &w.flags, now, Sao_Paulo, None, &id)
            .await
            .err(),
        Some(AppError::NotFound)
    );
    let edit = input(
        ProposalKind::NormalLaw,
        Level::Municipal,
        "Praça pública reformada",
    );
    assert_eq!(
        proposals::save_draft(&w.ctx(now), &other, Some(&id), &edit)
            .await
            .err(),
        Some(AppError::NotFound)
    );
    proposals::save_draft(&w.ctx(now), &author, Some(&id), &edit)
        .await
        .unwrap();
    assert_eq!(w.row(&id).await.title, "Praça pública reformada");
    assert_eq!(
        proposals::list(&conn, Sao_Paulo, &ListFilter::default())
            .await
            .unwrap()
            .total,
        0
    );

    let short = input(ProposalKind::NormalLaw, Level::Municipal, "x");
    assert!(matches!(
        proposals::save_draft(&w.ctx(now), &author, None, &short).await,
        Err(AppError::Validation { .. })
    ));
    let off = Flags::with(&[]);
    let ctx = Ctx {
        flags: &off,
        ..w.ctx(now)
    };
    assert!(matches!(
        proposals::save_draft(&ctx, &author, None, &edit).await,
        Err(AppError::FeatureDisabled(_))
    ));
}

#[tokio::test]
async fn publish_snapshots_electorate_consumes_reauth_and_freezes_text() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    for _ in 0..3 {
        w.citizen(now, "4202404").await;
    }
    w.citizen(now, "4205407").await; // other municipality: not in the electorate
    let id = w
        .draft(now, &author, ProposalKind::NormalLaw, "Ciclovias")
        .await;
    assert_eq!(
        proposals::publish(&w.ctx(now), &author, &id, None)
            .await
            .err(),
        Some(AppError::ReauthRequired)
    );
    let stranger = w.citizen(now, "4202404").await;
    w.grant(now, &stranger, proposals::REAUTH_PUBLISH, &id)
        .await;
    assert_eq!(
        proposals::publish(&w.ctx(now), &stranger, &id, None)
            .await
            .err(),
        Some(AppError::NotFound)
    );

    w.grant(now, &author, proposals::REAUTH_PUBLISH, &id).await;
    let p = proposals::publish(&w.ctx(now), &author, &id, None)
        .await
        .unwrap();
    assert_eq!(p.signature_target, 1, "ceil(2% × 5) = 1");
    assert_eq!(p.collect_ends_at, "2027-01-22T12:00:00.000Z", "90 days");
    let r = w.row(&id).await;
    assert_eq!(r.status, ProposalStatus::CollectingSignatures);
    let snap = r.collection_snapshot_id.clone().unwrap();
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM electorate_members WHERE snapshot_id = ?1",
            &snap
        )
        .await,
        5
    );
    assert_eq!(
        w.count(
            "SELECT member_count FROM electorate_snapshots WHERE id = ?1",
            &snap
        )
        .await,
        5
    );
    assert_eq!(
        r.text_hash.unwrap(),
        proposals::text_hash(&r.title, &r.summary, &r.body)
    );
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM audit_events WHERE kind = 'proposal.published' AND subject = ?1",
            &id
        )
        .await,
        1
    );
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ?1",
            &author.user_id
        )
        .await,
        1
    );

    // Replay without a new grant fails; the text is immutable at the database level.
    assert_eq!(
        proposals::publish(&w.ctx(now), &author, &id, None)
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );
    let conn = w.t.db.read().await.unwrap();
    assert!(
        conn.execute(
            "UPDATE proposals SET body = 'outro' WHERE id = ?1",
            [id.as_str()]
        )
        .await
        .is_err()
    );
    assert!(
        conn.execute("DELETE FROM proposals WHERE id = ?1", [id.as_str()])
            .await
            .is_err()
    );
    assert!(
        conn.execute(
            "DELETE FROM electorate_members WHERE snapshot_id = ?1",
            [snap.as_str()]
        )
        .await
        .is_err()
    );
    assert!(
        conn.execute(
            "UPDATE electorate_snapshots SET member_count = 9 WHERE id = ?1",
            [snap.as_str()]
        )
        .await
        .is_err()
    );
    let edit = input(ProposalKind::NormalLaw, Level::Municipal, "Ciclovias novas");
    assert_eq!(
        proposals::save_draft(&w.ctx(now), &author, Some(&id), &edit)
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );
}

#[tokio::test]
async fn publish_idempotency_key_replays_and_conflicts() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    let a = w
        .draft(now, &author, ProposalKind::NormalLaw, "Primeira proposta")
        .await;
    let b = w
        .draft(now, &author, ProposalKind::NormalLaw, "Segunda proposta")
        .await;
    w.grant(now, &author, proposals::REAUTH_PUBLISH, &a).await;
    let first = proposals::publish(&w.ctx(now), &author, &a, Some("key-0001"))
        .await
        .unwrap();
    let replay = proposals::publish(&w.ctx(now), &author, &a, Some("key-0001"))
        .await
        .unwrap();
    assert_eq!(
        first, replay,
        "same key + payload returns the stored result without a new grant"
    );
    assert!(matches!(
        proposals::publish(&w.ctx(now), &author, &b, Some("key-0001")).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        proposals::publish(&w.ctx(now), &author, &b, Some("short")).await,
        Err(AppError::Validation { .. })
    ));
}

#[tokio::test]
async fn yearly_quota_uses_instance_civil_year_and_is_not_refunded() {
    let w = world().await;
    let start = at("2026-10-24T12:00:00Z");
    w.adopt(start, &SUGGESTED).await;
    let author = w.citizen(start, "4202404").await;
    // 02:00Z on 1 Jan is still 31 Dec 23:00 in São Paulo.
    let late_2026 = at("2027-01-01T02:00:00Z");
    let first = w
        .publish(late_2026, &author, ProposalKind::UrgentLaw, "Urgente um")
        .await;
    proposals::withdraw(&w.ctx(late_2026), &author, &first, "desisti da proposta")
        .await
        .unwrap();
    assert_eq!(w.row(&first).await.status, ProposalStatus::Archived);
    let second = w
        .draft(late_2026, &author, ProposalKind::UrgentLaw, "Urgente dois")
        .await;
    w.grant(late_2026, &author, proposals::REAUTH_PUBLISH, &second)
        .await;
    let err = proposals::publish(&w.ctx(at("2027-01-01T02:59:00Z")), &author, &second, None).await;
    assert!(
        matches!(err, Err(AppError::Conflict(_))),
        "withdrawal does not refund: {err:?}"
    );
    let new_year = at("2027-01-01T03:00:00Z");
    w.grant(new_year, &author, proposals::REAUTH_PUBLISH, &second)
        .await;
    proposals::publish(&w.ctx(new_year), &author, &second, None)
        .await
        .unwrap();
    // Drafts never consume quota; normal laws have their own quota.
    w.publish(new_year, &author, ProposalKind::NormalLaw, "Normal um")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_publishes_do_not_exceed_quota() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    let mut ids = Vec::new();
    for i in 0..4 {
        let id = w
            .draft(
                now,
                &author,
                ProposalKind::UrgentLaw,
                &format!("Urgente número {i}"),
            )
            .await;
        w.grant(now, &author, proposals::REAUTH_PUBLISH, &id).await;
        ids.push(id);
    }
    let tasks: Vec<_> = ids
        .into_iter()
        .map(|id| {
            let (db, flags, author) = (w.t.db.clone(), w.flags.clone(), author.clone());
            tokio::spawn(async move {
                let ctx = Ctx {
                    db: &db,
                    flags: &flags,
                    now,
                    clock: None,
                    tz: Sao_Paulo,
                };
                proposals::publish(&ctx, &author, &id, None).await
            })
        })
        .collect();
    let mut ok = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(AppError::Conflict(_)) => {}
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert_eq!(ok, 1);
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM proposals WHERE author_id = ?1 AND quota_year = 2026",
            &author.user_id
        )
        .await,
        1
    );
}

#[tokio::test]
async fn signatures_eligibility_withdrawal_idempotency_and_threshold() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &all_must_sign()).await;
    let author = w.citizen(now, "4202404").await;
    let ana = w.citizen(now, "4202404").await;
    let outsider = w.citizen(now, "4205407").await;
    let id = w
        .publish(now, &author, ProposalKind::NormalLaw, "Biblioteca")
        .await;
    assert_eq!(w.row(&id).await.signature_target, Some(2));
    let late = w.citizen(now, "4202404").await; // registered after the snapshot

    assert_eq!(
        w.sign(now, &outsider, &id).await.err(),
        Some(AppError::Forbidden)
    );
    assert_eq!(
        w.sign(now, &late, &id).await.err(),
        Some(AppError::Forbidden)
    );
    assert_eq!(
        signatures::sign(&w.ctx(now), &ana, &id, None).await.err(),
        Some(AppError::ReauthRequired)
    );
    let off = Flags::with(&[PROPOSALS]);
    assert!(matches!(
        signatures::sign(
            &Ctx {
                flags: &off,
                ..w.ctx(now)
            },
            &ana,
            &id,
            None
        )
        .await,
        Err(AppError::FeatureDisabled(_))
    ));

    w.grant(now, &ana, signatures::REAUTH_SIGN, &id).await;
    let s = signatures::sign(&w.ctx(now), &ana, &id, Some("sign-key-1"))
        .await
        .unwrap();
    assert_eq!(
        (s.signature_count, s.status),
        (1, ProposalStatus::CollectingSignatures)
    );
    assert_eq!(
        signatures::sign(&w.ctx(now), &ana, &id, Some("sign-key-1"))
            .await
            .unwrap(),
        s,
        "replay"
    );
    assert!(
        matches!(w.sign(now, &ana, &id).await, Err(AppError::Conflict(_))),
        "one active signature"
    );
    let u = signatures::withdraw_signature(&w.ctx(now), &ana, &id, None)
        .await
        .unwrap();
    assert_eq!((u.signature_count, u.signed), (0, false));
    assert!(matches!(
        signatures::withdraw_signature(&w.ctx(now), &ana, &id, None).await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        w.sign(now, &ana, &id).await.unwrap().signature_count,
        1,
        "re-sign during collection"
    );
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM signatures WHERE proposal_id = ?1",
            &id
        )
        .await,
        1
    );

    let done = w.sign(now, &author, &id).await.unwrap();
    assert_eq!(
        (done.signature_count, done.status),
        (2, ProposalStatus::InDebate)
    );
    let r = w.row(&id).await;
    assert_eq!(
        r.debate_ends_at.as_deref(),
        Some("2026-11-08T12:00:00.000Z"),
        "15 days"
    );
    assert_eq!(
        signatures::withdraw_signature(&w.ctx(now), &ana, &id, None)
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );
    assert_eq!(w.count("SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'proposal.phase_changed'", &author.user_id).await, 1);
    let payloads = w.count("SELECT COUNT(*) FROM audit_events WHERE kind LIKE 'proposal.signature%' AND (actor_ref IS NOT NULL OR payload LIKE '%' || ?1 || '%')", &ana.user_id).await;
    assert_eq!(payloads, 0, "no signer identity in public audit events");
}

#[tokio::test]
async fn urgent_law_goes_straight_to_voting_with_snapshot() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &all_must_sign()).await;
    let author = w.citizen(now, "4202404").await;
    let id = w
        .publish(now, &author, ProposalKind::UrgentLaw, "Enchente")
        .await;
    w.citizen(now, "4202404").await; // joins after collection opened; counts for voting only
    let s = w.sign(now, &author, &id).await.unwrap();
    assert_eq!(s.status, ProposalStatus::InVoting);
    let r = w.row(&id).await;
    assert_eq!(
        r.vote_ends_at.as_deref(),
        Some("2026-10-31T12:00:00.000Z"),
        "7 days"
    );
    let snap = r.vote_snapshot_id.clone().unwrap();
    assert_eq!(
        w.count(
            "SELECT member_count FROM electorate_snapshots WHERE id = ?1",
            &snap
        )
        .await,
        2
    );
    let conn = w.t.db.read().await.unwrap();
    assert_eq!(
        phases::vote_rules(&conn, &r).await.unwrap(),
        (2, 2000, 5000)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_signatures_at_threshold_transition_once() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(
        now,
        &SetupParams {
            signature_bp_municipal: 5000,
            ..SUGGESTED
        },
    )
    .await;
    let author = w.citizen(now, "4202404").await;
    let mut signers = Vec::new();
    for _ in 0..7 {
        signers.push(w.citizen(now, "4202404").await);
    }
    let id = w
        .publish(now, &author, ProposalKind::NormalLaw, "Hospital")
        .await;
    assert_eq!(w.row(&id).await.signature_target, Some(4), "ceil(50% × 8)");
    for s in &signers {
        w.grant(now, s, signatures::REAUTH_SIGN, &id).await;
    }
    let tasks: Vec<_> = signers
        .into_iter()
        .map(|s| {
            let (db, flags, id) = (w.t.db.clone(), w.flags.clone(), id.clone());
            tokio::spawn(async move {
                let ctx = Ctx {
                    db: &db,
                    flags: &flags,
                    now,
                    clock: None,
                    tz: Sao_Paulo,
                };
                signatures::sign(&ctx, &s, &id, None).await
            })
        })
        .collect();
    let mut ok = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(AppError::PhaseClosed) => {}
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    let r = w.row(&id).await;
    assert_eq!(
        (ok, r.signature_count, r.status),
        (4, 4, ProposalStatus::InDebate)
    );
    assert_eq!(w.count("SELECT COUNT(*) FROM audit_events WHERE kind = 'proposal.phase_changed' AND subject = ?1", &id).await, 1);
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM signatures WHERE proposal_id = ?1 AND active = 1",
            &id
        )
        .await,
        4
    );
}

#[tokio::test]
async fn deadlines_expire_open_voting_and_resume_after_restart() {
    let mut w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &all_must_sign()).await;
    let author = w.citizen(now, "4202404").await;
    let ana = w.citizen(now, "4202404").await;
    let expiring = w
        .publish(now, &author, ProposalKind::NormalLaw, "Vai expirar")
        .await;
    let debated = w
        .publish(now, &author, ProposalKind::NormalLaw, "Vai a debate")
        .await;
    w.sign(now, &author, &debated).await.unwrap();
    w.sign(now, &ana, &debated).await.unwrap();
    assert_eq!(w.row(&debated).await.status, ProposalStatus::InDebate);

    let debate_end = now + Duration::days(15);
    let ms = Duration::milliseconds(1);
    assert!(
        phases::advance_due(&w.t.db, debate_end - ms)
            .await
            .unwrap()
            .is_empty()
    );

    // Restart: reopen the database file; persisted instants drive the transitions.
    let path = w.t.dir.path().join("test.db");
    w.t.db = Db::open(&path).await.unwrap();
    let reopened = w.t.db.clone();
    let applied = phases::advance_due(&reopened, debate_end).await.unwrap();
    assert_eq!(applied, vec![(debated.clone(), ProposalStatus::InVoting)]);
    // Withdrawal only in draft/collection; admin archive only from terminal phases.
    assert_eq!(
        proposals::withdraw(&w.ctx(debate_end), &author, &debated, "não quero mais")
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );
    let admin = w.admin(debate_end).await;
    assert_eq!(
        proposals::archive(&w.ctx(debate_end), &admin, &debated, "limpeza geral")
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );

    let deadline = now + Duration::days(90);
    // The 30-day vote (ends on day 45) is closed by the voting feature: no ballots.
    assert_eq!(
        phases::advance_due(&reopened, deadline - ms).await.unwrap(),
        vec![(debated.clone(), ProposalStatus::QuorumNotReached)]
    );
    assert_eq!(
        w.sign(deadline, &ana, &expiring).await.err(),
        Some(AppError::PhaseClosed),
        "closing wins at the deadline"
    );
    let applied = phases::advance_due(&reopened, deadline).await.unwrap();
    assert_eq!(applied, vec![(expiring.clone(), ProposalStatus::Expired)]);
    assert!(
        phases::advance_due(&reopened, deadline)
            .await
            .unwrap()
            .is_empty(),
        "idempotent"
    );
    let r = w.row(&debated).await;
    assert!(r.vote_snapshot_id.is_some() && r.vote_setup_version_id.is_some());
    assert!(
        phases::due_votes(
            &reopened.read().await.unwrap(),
            deadline + Duration::days(30)
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        proposals::archive(&w.ctx(deadline), &author, &expiring, "limpeza geral")
            .await
            .err(),
        Some(AppError::Forbidden)
    );
    proposals::archive(&w.ctx(deadline), &admin, &expiring, "limpeza geral")
        .await
        .unwrap();
    let r = w.row(&expiring).await;
    assert_eq!(
        (r.status, r.archive_reason.as_deref()),
        (ProposalStatus::Archived, Some("limpeza geral"))
    );
}

#[tokio::test]
async fn setup_change_vote_adopts_package_for_future_elections() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &all_must_sign()).await;
    let author = w.citizen(now, "4202404").await;
    let law = w
        .publish(now, &author, ProposalKind::NormalLaw, "Lei em coleta")
        .await;
    let law_setup = w.row(&law).await.setup_version_id;

    let meta = w
        .publish(now, &author, ProposalKind::SetupChange, "Nova regra")
        .await;
    w.sign(now, &author, &meta).await.unwrap();
    phases::advance_due(&w.t.db, now + Duration::days(15))
        .await
        .unwrap();
    let r = w.row(&meta).await;
    assert_eq!(r.status, ProposalStatus::InVoting);
    assert_eq!(
        r.vote_ends_at.as_deref(),
        Some("2026-12-08T12:00:00.000Z"),
        "30-day meta-law vote"
    );

    let later = now + Duration::days(45);
    let tx = w.t.db.write().await.unwrap();
    let calc = phases::close_vote(
        &tx,
        later,
        &meta,
        Tally {
            yes: 1,
            no: 0,
            abstain: 0,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(calc.outcome, Outcome::Approved);
    let conn = w.t.db.read().await.unwrap();
    let eff = setup::effective_setup(&conn, &w.bnu)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (eff.jurisdiction_id.as_str(), eff.version, eff.params),
        (w.bnu.as_str(), Some(1), SUGGESTED),
        "municipal meta-law adopts a local setup"
    );
    assert_eq!(eff.source_proposal_id.as_deref(), Some(meta.as_str()));
    assert_eq!(
        w.row(&law).await.setup_version_id,
        law_setup,
        "running election keeps its rules"
    );
    assert_eq!(setup::history(&conn).await.unwrap().len(), 2);
}

#[tokio::test]
async fn initial_setup_plebiscite_gates_laws_until_approved() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    let admin = w.admin(now).await;
    let citizen = w.citizen(now, "3550308").await;
    assert_eq!(
        phases::start_initial_setup(&w.t.db, now, &citizen, &SUGGESTED, "Setup", "Texto")
            .await
            .err(),
        Some(AppError::Forbidden)
    );
    let first = phases::start_initial_setup(
        &w.t.db,
        now,
        &admin,
        &SUGGESTED,
        "Plebiscito inicial",
        "Pacote sugerido",
    )
    .await
    .unwrap();
    assert!(matches!(
        phases::start_initial_setup(&w.t.db, now, &admin, &SUGGESTED, "Outro", "Outro").await,
        Err(AppError::Conflict(_))
    ));
    let r = w.row(&first).await;
    assert_eq!(r.status, ProposalStatus::InVoting);
    assert_eq!(
        w.count(
            "SELECT member_count FROM electorate_snapshots WHERE id = ?1",
            r.vote_snapshot_id.as_deref().unwrap()
        )
        .await,
        2,
        "national electorate"
    );

    let tx = w.t.db.write().await.unwrap();
    let calc = phases::close_vote(
        &tx,
        now,
        &first,
        Tally {
            yes: 0,
            no: 0,
            abstain: 0,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(calc.outcome, Outcome::QuorumNotReached);
    let blocked = proposals::save_draft(
        &w.ctx(now),
        &citizen,
        None,
        &input(ProposalKind::NormalLaw, Level::Municipal, "Bloqueada"),
    )
    .await;
    assert!(
        matches!(blocked, Err(AppError::Conflict(_))),
        "laws stay blocked after failure"
    );

    let second = phases::start_initial_setup(
        &w.t.db,
        now,
        &admin,
        &SUGGESTED,
        "Plebiscito inicial 2",
        "Pacote sugerido",
    )
    .await
    .unwrap();
    let tx = w.t.db.write().await.unwrap();
    assert_eq!(
        phases::close_vote(
            &tx,
            now,
            &second,
            Tally {
                yes: 2,
                no: 0,
                abstain: 0
            }
        )
        .await
        .unwrap()
        .outcome,
        Outcome::Approved
    );
    tx.commit().await.unwrap();
    proposals::save_draft(
        &w.ctx(now),
        &citizen,
        None,
        &input(ProposalKind::NormalLaw, Level::Municipal, "Agora pode"),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn listing_filters_paginates_and_warns_duplicates() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    for i in 0..3 {
        w.publish(
            now + Duration::seconds(i),
            &author,
            ProposalKind::NormalLaw,
            &format!("Lei normal {i}"),
        )
        .await;
    }
    w.publish(now, &author, ProposalKind::UrgentLaw, "Lei urgente")
        .await;
    w.draft(now, &author, ProposalKind::NormalLaw, "Rascunho oculto")
        .await;
    let conn = w.t.db.read().await.unwrap();
    let all = proposals::list(&conn, Sao_Paulo, &ListFilter::default())
        .await
        .unwrap();
    assert_eq!((all.total, all.pages), (4, 1));
    let urgent = ListFilter::parse(Some("urgent_law"), None, Some("municipal"), None).unwrap();
    assert_eq!(
        proposals::list(&conn, Sao_Paulo, &urgent)
            .await
            .unwrap()
            .items[0]
            .title,
        "Lei urgente"
    );
    let national = ListFilter::parse(None, None, Some("national"), None).unwrap();
    assert_eq!(
        proposals::list(&conn, Sao_Paulo, &national)
            .await
            .unwrap()
            .total,
        0
    );
    assert!(ListFilter::parse(None, Some("draft"), None, None).is_err());
    assert!(ListFilter::parse(Some("x"), None, None, None).is_err());

    let dup = proposals::save_draft(
        &w.ctx(now),
        &author,
        None,
        &input(ProposalKind::NormalLaw, Level::Municipal, "LEI normal, 0!"),
    )
    .await
    .unwrap();
    assert_eq!(
        dup.duplicates.len(),
        1,
        "normalized title matches; warning only"
    );
    let md = proposals::export_markdown(&conn, Sao_Paulo, &all.items[0].id)
        .await
        .unwrap();
    assert!(md.contains("SHA-256") && md.contains("Lei normal 2"));
    assert_eq!(
        proposals::export_markdown(&conn, Sao_Paulo, &dup.id)
            .await
            .err(),
        Some(AppError::NotFound),
        "drafts are not exported"
    );
}

#[tokio::test]
async fn proposal_author_name_respects_profile_visibility() {
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    let other = w.citizen(now, "4202404").await;
    let id = w
        .publish(now, &author, ProposalKind::NormalLaw, "Praça pública")
        .await;
    let label = async |viewer: Option<&Actor>| {
        let conn = w.t.db.read().await.unwrap();
        proposals::detail(&conn, &w.flags, now, Sao_Paulo, viewer, &id)
            .await
            .unwrap()
            .author
    };
    assert_eq!(label(None).await, "Cidadão(ã)", "private by default");
    assert_eq!(label(Some(&other)).await, "Cidadão(ã)");
    let tx = w.t.db.write().await.unwrap();
    tx.execute(
        "INSERT INTO user_preferences (user_id, profile_visibility, updated_at) VALUES (?1, 'public', 'now')",
        [author.user_id.as_str()],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(label(None).await, "Cidadã Sintética");
}

#[tokio::test]
async fn signature_deadline_uses_time_read_after_acquiring_the_writer() {
    use crate::infrastructure::clock::ManualClock;
    let w = world().await;
    let now = at("2026-10-24T12:00:00Z");
    w.adopt(now, &SUGGESTED).await;
    let author = w.citizen(now, "4202404").await;
    let ana = w.citizen(now, "4202404").await;
    let id = w
        .publish(now, &author, ProposalKind::NormalLaw, "Biblioteca")
        .await;
    let end = at(&w.row(&id).await.collect_ends_at.unwrap());
    w.grant(end, &ana, signatures::REAUTH_SIGN, &id).await;
    let clock = ManualClock::new(end - Duration::milliseconds(1));
    let ctx = Ctx {
        clock: Some(&clock),
        ..w.ctx(end - Duration::milliseconds(1))
    };
    clock.set(end + Duration::seconds(1));
    assert_eq!(
        signatures::sign(&ctx, &ana, &id, None).await.err(),
        Some(AppError::PhaseClosed)
    );
}
