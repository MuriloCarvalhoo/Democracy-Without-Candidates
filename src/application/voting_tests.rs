//! Demonstrative public vote against the real TursoDB engine: casting rules,
//! idempotency, concurrency, cast-vs-close race, restart-safe closing, result
//! persistence, notifications, initial plebiscite and statistics.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::America::Sao_Paulo;

use super::{
    actor::{self, Actor},
    notify, phases, plebiscite,
    proposals::{self, Ctx},
    reauth,
    session::{SessionPolicy, testing::citizen_with_session, token_hash},
    setup, signatures, statistics, voting,
};
use crate::{
    domain::{
        identity::Role,
        proposals::{
            Level, Outcome, ProposalKind, ProposalStatus, RejectReason, Tally, compute_result,
            view::DraftInput,
        },
        setup::{SUGGESTED, SetupParams},
        voting::{Choice, REAUTH_SETUP_VOTE, REAUTH_VOTE, view::ResultState},
    },
    error::AppError,
    infrastructure::{
        catalog,
        db::{
            Db, query_opt,
            testing::{TestDb, migrated},
        },
        flags::{Flags, PROPOSALS, SIGNATURES, VOTING},
    },
};

const CATALOG: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n35,state,São Paulo,BR,SP\n3550308,municipal,São Paulo,35,SP\n";
const BNU: &str = "4202404";

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn t0() -> DateTime<Utc> {
    at("2026-10-24T12:00:00Z")
}

struct W {
    t: TestDb,
    flags: Flags,
}

/// Signature target of one account (bp 1), so the author's own signature moves
/// an urgent law straight to voting (7 days) over the current electorate.
fn quick() -> SetupParams {
    SetupParams {
        signature_bp_municipal: 1,
        signature_bp_national: 1,
        setup_debate_days: 1,
        ..SUGGESTED
    }
}

async fn world(params: Option<SetupParams>) -> W {
    let t = migrated().await;
    catalog::import_str(&t.db, "mini", CATALOG).await.unwrap();
    if let Some(params) = params {
        let br: String = query_opt(
            &t.db.read().await.unwrap(),
            "SELECT id FROM jurisdictions WHERE code = 'BR'",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        let tx = t.db.write().await.unwrap();
        setup::adopt_setup(&tx, t0(), &br, &params, None, true)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    W {
        t,
        flags: Flags::with(&[PROPOSALS, SIGNATURES, VOTING]),
    }
}

impl W {
    fn ctx(&self, now: DateTime<Utc>) -> Ctx<'_> {
        Ctx {
            db: &self.t.db,
            flags: &self.flags,
            now,
            clock: None,
            tz: Sao_Paulo,
        }
    }

    async fn citizen(&self, code: &str) -> Actor {
        let (user, token) =
            citizen_with_session(&self.t.db, t0(), SessionPolicy::from_secs(900, 28800), code)
                .await;
        actor::load(&self.t.db.read().await.unwrap(), &user, &token_hash(&token))
            .await
            .unwrap()
            .unwrap()
    }

    async fn grant(&self, now: DateTime<Utc>, a: &Actor, action: &str, id: &str) {
        let tx = self.t.db.write().await.unwrap();
        reauth::grant(&tx, now, &a.session_hash, action, id)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    /// Published + signed by the author: in voting until `t0 + 7 days` (urgent).
    async fn urgent_in_voting(&self, author: &Actor, title: &str) -> String {
        let now = t0();
        let input = DraftInput {
            kind: ProposalKind::UrgentLaw,
            level: Level::Municipal,
            title: title.into(),
            summary: "Resumo sintético da proposta".into(),
            body: format!("Texto completo sintético para {title}."),
            proposed_setup: None,
        };
        let id = proposals::save_draft(&self.ctx(now), author, None, &input)
            .await
            .unwrap()
            .id;
        self.grant(now, author, proposals::REAUTH_PUBLISH, &id)
            .await;
        proposals::publish(&self.ctx(now), author, &id, None)
            .await
            .unwrap();
        self.grant(now, author, signatures::REAUTH_SIGN, &id).await;
        let s = signatures::sign(&self.ctx(now), author, &id, None)
            .await
            .unwrap();
        assert_eq!(s.status, ProposalStatus::InVoting);
        id
    }

    async fn vote(
        &self,
        now: DateTime<Utc>,
        a: &Actor,
        id: &str,
        choice: Choice,
    ) -> Result<crate::domain::voting::view::CastConfirmation, AppError> {
        self.grant(now, a, REAUTH_VOTE, id).await;
        voting::cast(&self.ctx(now), a, id, choice, true, None).await
    }

    async fn count(&self, sql: &str, param: &str) -> i64 {
        query_opt(&self.t.db.read().await.unwrap(), sql, [param])
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }
}

fn ends() -> DateTime<Utc> {
    t0() + Duration::days(7)
}

#[tokio::test]
async fn cast_checks_flag_consent_eligibility_reauth_and_deadline() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let ana = w.citizen(BNU).await;
    let outsider = w.citizen("3550308").await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    let late = w.citizen(BNU).await; // joined after the snapshot
    let now = t0() + Duration::hours(1);

    let off = Flags::with(&[PROPOSALS, SIGNATURES]);
    let ctx = Ctx {
        flags: &off,
        ..w.ctx(now)
    };
    assert_eq!(
        voting::cast(&ctx, &ana, &id, Choice::Yes, true, None).await,
        Err(AppError::FeatureDisabled(VOTING.into()))
    );
    w.grant(now, &ana, REAUTH_VOTE, &id).await;
    assert!(matches!(
        voting::cast(&w.ctx(now), &ana, &id, Choice::Yes, false, None).await,
        Err(AppError::Validation { .. })
    ));
    for who in [&outsider, &late] {
        w.grant(now, who, REAUTH_VOTE, &id).await;
        assert_eq!(
            voting::cast(&w.ctx(now), who, &id, Choice::Yes, true, None).await,
            Err(AppError::Forbidden),
            "not in the voting snapshot"
        );
    }
    // Reauth bound to the action: a setup-vote grant does not authorize a law vote.
    w.grant(now, &author, REAUTH_SETUP_VOTE, &id).await;
    assert_eq!(
        voting::cast(&w.ctx(now), &author, &id, Choice::No, true, None).await,
        Err(AppError::ReauthRequired)
    );

    let ok = voting::cast(
        &w.ctx(now),
        &ana,
        &id,
        Choice::Yes,
        true,
        Some("attempt-0001"),
    )
    .await
    .unwrap();
    assert_eq!(ok.choice, Choice::Yes);
    assert_eq!(
        voting::cast(
            &w.ctx(now),
            &ana,
            &id,
            Choice::Yes,
            true,
            Some("attempt-0001")
        )
        .await,
        Ok(ok.clone()),
        "replay of the same attempt"
    );
    assert!(matches!(
        voting::cast(
            &w.ctx(now),
            &ana,
            &id,
            Choice::No,
            true,
            Some("attempt-0001")
        )
        .await,
        Err(AppError::Conflict(_))
    ));
    w.grant(now, &ana, REAUTH_VOTE, &id).await;
    let again = voting::cast(
        &w.ctx(now),
        &ana,
        &id,
        Choice::No,
        true,
        Some("attempt-0002"),
    )
    .await;
    assert!(
        matches!(&again, Err(AppError::Conflict(m)) if m.contains("já registrado")),
        "{again:?}"
    );
    let conn = w.t.db.read().await.unwrap();
    let status = voting::status(&conn, Sao_Paulo, &ana, &id, Some("attempt-0001"))
        .await
        .unwrap();
    assert!(status.attempt_recorded);
    assert_eq!(status.ballot.unwrap().choice, Choice::Yes);
    assert!(
        !voting::status(&conn, Sao_Paulo, &ana, &id, Some("never-sent"))
            .await
            .unwrap()
            .attempt_recorded
    );

    assert_eq!(
        w.vote(ends(), &author, &id, Choice::Yes).await,
        Err(AppError::PhaseClosed),
        "closing wins at the deadline"
    );
    assert_eq!(
        w.count("SELECT COUNT(*) FROM ballots WHERE proposal_id = ?1", &id)
            .await,
        1
    );
    // Public audit event: running count only, no account or choice.
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM audit_events WHERE kind = 'vote.cast' AND subject = ?1 AND actor_ref IS NULL AND payload NOT LIKE '%yes%'",
            &id
        )
        .await,
        1
    );
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM audit_events WHERE payload LIKE '%' || ?1 || '%'",
            &ana.user_id
        )
        .await,
        0
    );
}

#[tokio::test]
async fn ballots_are_public_append_only_in_the_database() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    w.vote(t0(), &author, &id, Choice::Abstain).await.unwrap();
    let conn = w.t.db.read().await.unwrap();
    assert!(
        conn.execute("UPDATE ballots SET choice = 'yes'", ())
            .await
            .is_err()
    );
    assert!(conn.execute("DELETE FROM ballots", ()).await.is_err());
    let snap = proposals::load(&conn, &id)
        .await
        .unwrap()
        .unwrap()
        .vote_snapshot_id
        .unwrap();
    assert!(
        conn.execute(
            "INSERT INTO ballots (id, proposal_id, user_id, snapshot_id, choice, visibility, consented_at, cast_at)
             VALUES ('x', ?1, ?2, ?3, 'yes', 'private', 'now', 'now')",
            (id.as_str(), author.user_id.as_str(), snap.as_str()),
        )
        .await
        .is_err(),
        "only public ballots exist in this build"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_casts_by_the_same_voter_accept_one() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let ana = w.citizen(BNU).await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    for _ in 0..4 {
        w.grant(t0(), &ana, REAUTH_VOTE, &id).await;
    }
    let tasks: Vec<_> = (0..4)
        .map(|i| {
            let (db, flags, id, ana) = (w.t.db.clone(), w.flags.clone(), id.clone(), ana.clone());
            tokio::spawn(async move {
                let ctx = Ctx {
                    db: &db,
                    flags: &flags,
                    now: t0(),
                    clock: None,
                    tz: Sao_Paulo,
                };
                let choice = if i % 2 == 0 { Choice::Yes } else { Choice::No };
                voting::cast(
                    &ctx,
                    &ana,
                    &id,
                    choice,
                    true,
                    Some(&format!("concurrent-{i}")),
                )
                .await
            })
        })
        .collect();
    let mut ok = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => ok += 1,
            Err(AppError::Conflict(_)) => {}
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert_eq!(ok, 1);
    assert_eq!(
        w.count("SELECT COUNT(*) FROM ballots WHERE proposal_id = ?1", &id)
            .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cast_racing_close_is_counted_or_rejected_never_lost() {
    let w = world(Some(quick())).await;
    let mut outcomes = (0, 0);
    for i in 0..6 {
        let voter = w.citizen(BNU).await;
        let author = w.citizen(BNU).await;
        let id = w.urgent_in_voting(&author, &format!("Corrida {i}")).await;
        let deadline = ends();
        w.grant(deadline - Duration::minutes(1), &voter, REAUTH_VOTE, &id)
            .await;
        let cast = {
            let (db, flags, id) = (w.t.db.clone(), w.flags.clone(), id.clone());
            tokio::spawn(async move {
                let ctx = Ctx {
                    db: &db,
                    flags: &flags,
                    now: deadline - Duration::milliseconds(1),
                    clock: None,
                    tz: Sao_Paulo,
                };
                voting::cast(&ctx, &voter, &id, Choice::Yes, true, None).await
            })
        };
        let close = {
            let db = w.t.db.clone();
            tokio::spawn(async move { voting::close_due_votes(&db, deadline).await })
        };
        let cast = cast.await.unwrap();
        close.await.unwrap().unwrap();
        // Whatever the order, a closing pass after both finished leaves it closed.
        voting::close_due_votes(&w.t.db, deadline).await.unwrap();
        let yes = w
            .count(
                "SELECT yes_count FROM vote_results WHERE proposal_id = ?1",
                &id,
            )
            .await;
        let ballots = w
            .count("SELECT COUNT(*) FROM ballots WHERE proposal_id = ?1", &id)
            .await;
        match cast {
            Ok(_) => {
                assert_eq!((yes, ballots), (1, 1), "accepted ballot is counted");
                outcomes.0 += 1;
            }
            Err(AppError::PhaseClosed) => {
                assert_eq!((yes, ballots), (0, 0), "rejected ballot is not stored");
                outcomes.1 += 1;
            }
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
    assert_eq!(outcomes.0 + outcomes.1, 6);
}

#[tokio::test]
async fn closing_is_idempotent_restart_safe_and_matches_compute_result() {
    let mut w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let voters = [
        w.citizen(BNU).await,
        w.citizen(BNU).await,
        w.citizen(BNU).await,
    ];
    let id = w.urgent_in_voting(&author, "Enchente").await;
    let now = t0() + Duration::days(1);
    w.vote(now, &author, &id, Choice::Yes).await.unwrap();
    w.vote(now, &voters[0], &id, Choice::Yes).await.unwrap();
    w.vote(now, &voters[1], &id, Choice::No).await.unwrap();
    w.vote(now, &voters[2], &id, Choice::Abstain).await.unwrap();

    // Result page: no partial counts while open.
    let conn = w.t.db.read().await.unwrap();
    let open = voting::result(&conn, now, Sao_Paulo, true, &id)
        .await
        .unwrap();
    assert!(matches!(
        open.state,
        ResultState::Open { electorate: 4, .. }
    ));
    drop(conn);

    let ms = Duration::milliseconds(1);
    assert!(
        voting::close_due_votes(&w.t.db, ends() - ms)
            .await
            .unwrap()
            .is_empty()
    );
    // Restart: reopen the file; persisted instants and ballots drive the closing.
    let path = w.t.dir.path().join("test.db");
    w.t.db = Db::open(&path).await.unwrap();
    let applied = phases::advance_due(&w.t.db, ends()).await.unwrap();
    assert_eq!(applied, vec![(id.clone(), ProposalStatus::Approved)]);
    assert!(
        phases::advance_due(&w.t.db, ends() + Duration::days(1))
            .await
            .unwrap()
            .is_empty(),
        "idempotent"
    );
    let tally = Tally {
        yes: 2,
        no: 1,
        abstain: 1,
    };
    let expected = compute_result(tally, 4, 2000, 5000).unwrap();
    let conn = w.t.db.read().await.unwrap();
    let page = voting::result(&conn, ends(), Sao_Paulo, true, &id)
        .await
        .unwrap();
    let ResultState::Closed(r) = page.state else {
        panic!("closed result expected: {:?}", page.state)
    };
    assert_eq!(r.tally, tally);
    assert_eq!(
        (
            r.electorate,
            r.quorum_required,
            r.majority_required,
            r.outcome
        ),
        (
            4,
            expected.quorum_required,
            expected.majority_required,
            expected.outcome
        )
    );
    assert_eq!(r.setup_version, Some(1));
    assert_eq!(r.package, None);
    // Result notification scheduled for the jurisdiction; reminder already due/delivered or cancelled.
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM scheduled_notifications WHERE dedupe_key = 'vote-result:' || ?1",
            &id
        )
        .await,
        1
    );
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM audit_events WHERE kind = 'proposal.phase_changed' AND subject = ?1 AND payload LIKE '%approved%'",
            &id
        )
        .await,
        1
    );
}

#[tokio::test]
async fn tie_zero_valid_and_quorum_reasons_are_stored() {
    let w = world(Some(quick())).await;
    let a = w.citizen(BNU).await;
    let b = w.citizen(BNU).await;
    let c = w.citizen(BNU).await;
    let d = w.citizen(BNU).await;
    let tie = w.urgent_in_voting(&a, "Empate").await;
    let blank = w.urgent_in_voting(&b, "Só abstenções").await;
    let empty = w.urgent_in_voting(&c, "Sem quórum").await;
    let now = t0() + Duration::hours(2);
    w.vote(now, &a, &tie, Choice::Yes).await.unwrap();
    w.vote(now, &b, &tie, Choice::No).await.unwrap();
    w.vote(now, &a, &blank, Choice::Abstain).await.unwrap();
    w.vote(now, &d, &blank, Choice::Abstain).await.unwrap();
    // Electorate 4: urgent quorum 20% → ceil(0.8) = 1; `empty` gets no vote.
    let closed = voting::close_due_votes(&w.t.db, ends()).await.unwrap();
    assert_eq!(closed.len(), 3);
    let conn = w.t.db.read().await.unwrap();
    let outcome = |id: String| {
        let conn = &conn;
        async move {
            match voting::result(conn, ends(), Sao_Paulo, true, &id)
                .await
                .unwrap()
                .state
            {
                ResultState::Closed(r) => r.outcome,
                other => panic!("{other:?}"),
            }
        }
    };
    assert_eq!(outcome(tie).await, Outcome::Rejected(RejectReason::Tie));
    assert_eq!(
        outcome(blank).await,
        Outcome::Rejected(RejectReason::NoValidVotes)
    );
    assert_eq!(outcome(empty).await, Outcome::QuorumNotReached);
}

#[tokio::test]
async fn opening_notifies_jurisdiction_and_schedules_the_reminder() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let ana = w.citizen(BNU).await;
    let sp = w.citizen("3550308").await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    let due: String = query_opt(
        &w.t.db.read().await.unwrap(),
        "SELECT due_at FROM scheduled_notifications WHERE dedupe_key = 'vote-24h:' || ?1",
        [id.as_str()],
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
    .unwrap();
    assert_eq!(
        due, "2026-10-30T12:00:00.000Z",
        "24 h before the 7-day vote ends"
    );
    notify::remind_due(&w.t.db, t0()).await.unwrap();
    let opened =
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'voting_opened'";
    assert_eq!(w.count(opened, &ana.user_id).await, 1);
    assert_eq!(w.count(opened, &sp.user_id).await, 0, "other municipality");
    notify::remind_due(&w.t.db, ends() - Duration::hours(24))
        .await
        .unwrap();
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'deadline_reminder'",
            &ana.user_id
        )
        .await,
        1
    );
    voting::close_due_votes(&w.t.db, ends()).await.unwrap();
    notify::remind_due(&w.t.db, ends()).await.unwrap();
    assert_eq!(
        w.count(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND event = 'result_published'",
            &ana.user_id
        )
        .await,
        1
    );
}

#[tokio::test]
async fn initial_plebiscite_is_started_by_admin_and_voted_with_setup_reauth() {
    let w = world(None).await;
    let admin = {
        let a = w.citizen(BNU).await;
        let tx = w.t.db.write().await.unwrap();
        tx.execute(
            "UPDATE user_roles SET role = 'COUNTRY_ADMIN' WHERE user_id = ?1",
            [a.user_id.as_str()],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        Actor {
            roles: vec![Role::CountryAdmin],
            ..a
        }
    };
    let ana = w.citizen(BNU).await;
    let bia = w.citizen("3550308").await;
    let none = Flags::with(&[PROPOSALS]);
    assert_eq!(
        plebiscite::start(&w.t.db, &none, t0(), &admin).await,
        Err(AppError::FeatureDisabled(VOTING.into()))
    );
    assert_eq!(
        plebiscite::start(&w.t.db, &w.flags, t0(), &ana).await,
        Err(AppError::Forbidden)
    );
    let conn = w.t.db.read().await.unwrap();
    assert!(
        plebiscite::overview(&conn, Sao_Paulo, &admin.country_id, Some(&admin))
            .await
            .unwrap()
            .can_start
    );
    let id = plebiscite::start(&w.t.db, &w.flags, t0(), &admin)
        .await
        .unwrap();
    let ov = plebiscite::overview(&conn, Sao_Paulo, &admin.country_id, Some(&admin))
        .await
        .unwrap();
    assert!(!ov.can_start && ov.open_plebiscite.is_some());
    assert!(matches!(
        plebiscite::start(&w.t.db, &w.flags, t0(), &admin).await,
        Err(AppError::Conflict(_))
    ));
    // Admin role alone does not vote; citizens vote with the setup action.
    w.grant(t0(), &admin, REAUTH_SETUP_VOTE, &id).await;
    assert_eq!(
        voting::cast(&w.ctx(t0()), &admin, &id, Choice::Yes, true, None).await,
        Err(AppError::Forbidden)
    );
    w.grant(t0(), &ana, REAUTH_VOTE, &id).await;
    assert_eq!(
        voting::cast(&w.ctx(t0()), &ana, &id, Choice::Yes, true, None).await,
        Err(AppError::ReauthRequired),
        "law action does not authorize a setup vote"
    );
    for who in [&ana, &bia] {
        w.grant(t0(), who, REAUTH_SETUP_VOTE, &id).await;
        voting::cast(&w.ctx(t0()), who, &id, Choice::Yes, true, None)
            .await
            .unwrap();
    }
    let end = t0() + Duration::days(30);
    assert_eq!(
        voting::close_due_votes(&w.t.db, end).await.unwrap(),
        vec![(id.clone(), ProposalStatus::Approved)]
    );
    let conn = w.t.db.read().await.unwrap();
    let ov = plebiscite::overview(&conn, Sao_Paulo, &admin.country_id, Some(&admin))
        .await
        .unwrap();
    assert_eq!(ov.effective_version, Some(1));
    assert!(!ov.can_start, "a new plebiscite only without adopted setup");
    let ResultState::Closed(r) = voting::result(&conn, end, Sao_Paulo, true, &id)
        .await
        .unwrap()
        .state
    else {
        panic!()
    };
    assert_eq!(r.package, Some(Some(1)), "package adopted as version 1");
    let diff = plebiscite::diff(&conn, &id).await.unwrap();
    assert_eq!((diff.current, diff.proposed), (None, SUGGESTED));
}

#[tokio::test]
async fn meta_law_vote_needs_setup_reauth_and_shows_parameter_diff() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let now = t0();
    let proposed = SetupParams {
        normal_quorum_bp: 3000,
        ..quick()
    };
    let input = DraftInput {
        kind: ProposalKind::SetupChange,
        level: Level::Municipal,
        title: "Quórum maior".into(),
        summary: "Resumo sintético da proposta".into(),
        body: "Texto completo sintético para a meta-lei.".into(),
        proposed_setup: Some(proposed),
    };
    let id = proposals::save_draft(&w.ctx(now), &author, None, &input)
        .await
        .unwrap()
        .id;
    w.grant(now, &author, proposals::REAUTH_PUBLISH, &id).await;
    proposals::publish(&w.ctx(now), &author, &id, None)
        .await
        .unwrap();
    w.grant(now, &author, signatures::REAUTH_SIGN, &id).await;
    signatures::sign(&w.ctx(now), &author, &id, None)
        .await
        .unwrap();
    phases::advance_due(&w.t.db, now + Duration::days(1))
        .await
        .unwrap();
    let conn = w.t.db.read().await.unwrap();
    let diff = plebiscite::diff(&conn, &id).await.unwrap();
    assert_eq!(diff.current, Some(quick()));
    assert_eq!(diff.proposed.normal_quorum_bp, 3000);
    let later = now + Duration::days(2);
    w.grant(later, &author, REAUTH_VOTE, &id).await;
    assert_eq!(
        voting::cast(&w.ctx(later), &author, &id, Choice::Yes, true, None).await,
        Err(AppError::ReauthRequired)
    );
    w.grant(later, &author, REAUTH_SETUP_VOTE, &id).await;
    voting::cast(&w.ctx(later), &author, &id, Choice::Yes, true, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn statistics_suppress_small_territorial_cells() {
    let w = world(Some(quick())).await;
    for _ in 0..12 {
        w.citizen(BNU).await;
    }
    for _ in 0..3 {
        w.citizen("3550308").await;
    }
    let author = w.citizen(BNU).await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    w.vote(t0(), &author, &id, Choice::Yes).await.unwrap();
    voting::close_due_votes(&w.t.db, ends()).await.unwrap();
    let s = statistics::statistics(&w.t.db.read().await.unwrap())
        .await
        .unwrap();
    let sc = s
        .citizens_by_state
        .iter()
        .find(|c| c.label == "Santa Catarina")
        .unwrap();
    let sp = s
        .citizens_by_state
        .iter()
        .find(|c| c.label == "São Paulo")
        .unwrap();
    assert_eq!((sc.count, sp.count), (Some(13), None));
    assert_eq!(s.citizens_total, None, "total would reveal the 3 in SP");
    assert_eq!(s.closed_votes.len(), 1);
    assert_eq!(
        (
            s.closed_votes[0].electorate,
            s.closed_votes[0].participation
        ),
        (13, 1)
    );
    assert!(
        s.by_status.contains(&(ProposalStatus::QuorumNotReached, 1)),
        "13 voters, quorum ceil(20% × 13) = 3"
    );
}

#[tokio::test]
async fn cast_deadline_uses_time_read_after_acquiring_the_writer() {
    use crate::infrastructure::clock::ManualClock;
    let w = world(Some(quick())).await;
    let voter = w.citizen(BNU).await;
    let author = w.citizen(BNU).await;
    let id = w.urgent_in_voting(&author, "Relógio").await;
    w.grant(ends() - Duration::minutes(1), &voter, REAUTH_VOTE, &id)
        .await;
    // Context built just before the deadline; the writer is only acquired after it.
    let clock = ManualClock::new(ends() - Duration::milliseconds(1));
    let ctx = Ctx {
        clock: Some(&clock),
        ..w.ctx(ends() - Duration::milliseconds(1))
    };
    clock.set(ends() + Duration::seconds(1));
    assert_eq!(
        voting::cast(&ctx, &voter, &id, Choice::Yes, true, None)
            .await
            .err(),
        Some(AppError::PhaseClosed)
    );
    assert_eq!(
        w.count("SELECT COUNT(*) FROM ballots WHERE proposal_id = ?1", &id)
            .await,
        0
    );
}

/// `DWC_DEBUG` traces the vote use case without the ballot choice or the
/// idempotency key (a choice must never be linkable to a user in logs).
#[tokio::test]
async fn debug_trace_of_cast_omits_choice_and_idempotency_key() {
    let w = world(Some(quick())).await;
    let author = w.citizen(BNU).await;
    let ana = w.citizen(BNU).await;
    let id = w.urgent_in_voting(&author, "Enchente").await;
    let now = t0() + Duration::hours(1);
    w.grant(now, &ana, REAUTH_VOTE, &id).await;

    let (_guard, out) = crate::server::observability::testing::capture("dwc=debug");
    voting::cast(
        &w.ctx(now),
        &ana,
        &id,
        Choice::Abstain,
        true,
        Some("chave-idem-secreta-123"),
    )
    .await
    .unwrap();
    let text = out.text();
    assert!(text.contains("cast") && text.contains(&id), "{text}");
    for secret in ["chave-idem-secreta-123", "abstain", "Abstain", "choice"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
}
