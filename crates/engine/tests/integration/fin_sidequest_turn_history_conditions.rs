//! FIN Sidequest turn-history conditions + the transform-then-attach anaphor.
//!
//! Two transform DFCs whose end-step/end-of-combat intervening-if clauses were
//! swallowed (`Swallow:Duration_ThisTurn`), plus the anaphor that binds the
//! second sentence's bare attachment pronoun to the source:
//!
//!   - Sidequest: Play Blitzball (`fin/158`) — "At the end of combat on your
//!     turn, if a player was dealt 6 or more combat damage this turn, transform
//!     this enchantment, then attach it to a creature you control."
//!     U1 (the `combat` damage-kind axis, CR 120.2a) + U3 (the
//!     `Transform{SelfRef}` → `Attach.attachment = SelfRef` anaphor, CR 608.2c).
//!   - Sidequest: Hunt the Mark (`fin/119`) — "At the beginning of your end
//!     step, if a creature died under an opponent's control this turn, create a
//!     Treasure token. Then if you control three or more Treasures, transform
//!     this enchantment."
//!     U2 (the opponent possessor axis on the dies condition, CR 608.2h).
//!
//! CR set (each verified against `docs/MagicCompRules.txt` before writing):
//! CR 109.4 + CR 109.5 (control and "you/your"), CR 120.1 + CR 120.2a +
//! CR 120.2b + CR 120.3 (damage, combat/noncombat, results), CR 301.5
//! (Equipment attaches to a creature), CR 603.3d (a trigger with no legal
//! choice is removed), CR 603.4 (intervening-if: checked at fire AND
//! resolution), CR 608.2c (follow instructions in order; later text may modify
//! earlier text), CR 608.2h (last-known information for the dead permanent's
//! controller), CR 608.2i (turn look-back), CR 700.4 (dies = battlefield to
//! graveyard), CR 701.3a (attach), CR 701.27a (transform).
//!
//! Oracle text is verbatim from Scryfall and byte-identical to the local
//! regenerated export (`client/public/card-data.json`). The file mirrors
//! `l02_bb4_intervening_if.rs` (parse fidelity rows with paired reach-guards +
//! discriminating runtime rows through the real trigger pipeline) and
//! `issue_605_calming_licid.rs` (attach `attachment`/`attached_to`/host
//! `attachments` assertions).
//!
//! Negative rows are paired with positive reach-guards: every "does not fire" /
//! "not attached" assertion is preceded by a proof that the path was reached
//! (the damage happened, the victim died, the parse produced the typed clause).

use engine::game::game_object::AttachTarget;
use engine::game::printed_cards::snapshot_object_face;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::parser::oracle::parse_oracle_text;
use engine::parser::oracle_ir::diagnostic::OracleDiagnostic;
use engine::types::ability::{
    AggregateFunction, Comparator, ControllerRef, DamageChannel, DamageKindFilter, Effect,
    FilterProp, QuantityExpr, QuantityRef, TargetFilter, TargetRef, TriggerCondition,
    TriggerConstraint, TypeFilter,
};
use engine::types::actions::GameAction;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::mana::ManaCost;
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::triggers::TriggerMode;
use engine::types::zones::Zone;

use super::rules::run_combat;

// ---------------------------------------------------------------------------
// Verbatim Oracle text (Scryfall + the local export, 2026-09-20)
// ---------------------------------------------------------------------------

const PLAY_BLITZBALL: &str = "At the beginning of combat on your turn, target creature you control gets +2/+0 until end of turn.\nAt the end of combat on your turn, if a player was dealt 6 or more combat damage this turn, transform this enchantment, then attach it to a creature you control.";

const HUNT_THE_MARK: &str = "When this enchantment enters, destroy up to one target creature.\nAt the beginning of your end step, if a creature died under an opponent's control this turn, create a Treasure token. Then if you control three or more Treasures, transform this enchantment.";

/// The real back face of Sidequest: Play Blitzball (`face_index 1`,
/// `Legendary Artifact — Equipment`). CR 701.27a: the transform is what makes
/// the source attachable at all (CR 301.5).
const WORLD_CHAMPION: &str = "Double Overdrive — Equipped creature gets +2/+0 and has double strike.\nEquip {3} ({3}: Attach to target creature you control. Equip only as a sorcery.)";

const DESTROY: &str = "Destroy target creature.";

const TREASURE_TOKEN: &str = "{T}, Sacrifice this token: Add one mana of any color.";

// ---------------------------------------------------------------------------
// Parse helpers (the `l02_bb4_intervening_if.rs` idiom)
// ---------------------------------------------------------------------------

fn parse_card(oracle: &str, name: &str) -> engine::parser::oracle::ParsedAbilities {
    parse_oracle_text(oracle, name, &[], &["Enchantment".to_string()], &[])
}

/// True when the parse produced a `SwallowedClause` diagnostic with `detector`.
fn has_swallowed(oracle: &str, name: &str, detector: &str) -> bool {
    parse_card(oracle, name).parse_warnings.iter().any(|w| {
        matches!(
            w,
            OracleDiagnostic::SwallowedClause { detector: d, .. } if d == detector
        )
    })
}

// ===========================================================================
// P1 — Sidequest: Play Blitzball (parse fidelity)
// ===========================================================================
//
// V4: the EndCombat trigger carries the exact combat-only threshold condition,
// BOTH swallows are cleared, and the "then attach it to a creature you control"
// clause survives with `attachment == SelfRef` (before U1 the condition was
// dropped and the clause tree was only `Transform`; with the condition but
// without U3 the clause survives with `attachment == ParentTarget` — the
// operand assertion, not the sub-ability's existence, discriminates U3).

#[test]
fn play_blitzball_parse_carries_combat_condition_and_self_ref_attach() {
    let parsed = parse_card(PLAY_BLITZBALL, "Sidequest: Play Blitzball");
    let trigger = parsed
        .triggers
        .iter()
        .find(|t| t.mode == TriggerMode::Phase && t.phase == Some(Phase::EndCombat))
        .expect("Play Blitzball must carry an EndCombat phase trigger");

    assert_eq!(
        trigger.condition,
        Some(TriggerCondition::QuantityComparison {
            lhs: QuantityExpr::Ref {
                qty: QuantityRef::DamageDealtThisTurn {
                    source: Box::new(TargetFilter::Any),
                    target: Box::new(TargetFilter::Player),
                    aggregate: AggregateFunction::Sum,
                    group_by: None,
                    damage_kind: DamageKindFilter::CombatOnly,
                    channel: DamageChannel::Total,
                },
            },
            comparator: Comparator::GE,
            rhs: QuantityExpr::Fixed { value: 6 },
        }),
        "CR 603.4 + CR 120.2a: the intervening-if must be the combat-only \
         player-damage threshold (\"a player was dealt 6 or more combat damage \
         this turn\")"
    );
    assert_eq!(
        trigger.constraint,
        Some(TriggerConstraint::OnlyDuringYourTurn),
        "\"on your turn\" is a typed trigger constraint, not part of the condition"
    );

    // Reach-guards for the two swallow negatives below: the clause tree survived
    // the condition extraction (otherwise the negatives pass vacuously — the
    // swallow checker returns early on `Effect::Unimplemented`).
    let execute = trigger
        .execute
        .as_ref()
        .expect("the EndCombat trigger has an execute body");
    match &*execute.effect {
        Effect::Transform {
            target: TargetFilter::SelfRef,
            ..
        } => {}
        other => panic!("expected `transform this enchantment` = Transform SelfRef, got {other:?}"),
    }
    let sub = execute
        .sub_ability
        .as_ref()
        .expect("the transform must chain the \"then attach it\" sub-ability");
    match &*sub.effect {
        Effect::Attach { attachment, target } => {
            assert_eq!(
                *attachment,
                TargetFilter::SelfRef,
                "CR 608.2c + CR 701.3a: the bare-pronoun attachment names the \
                 source the previous clause transformed"
            );
            match target {
                TargetFilter::Typed(tf) => {
                    assert!(
                        tf.type_filters.contains(&TypeFilter::Creature),
                        "the host is \"a creature you control\", got {:?}",
                        tf.type_filters
                    );
                    assert_eq!(tf.controller, Some(ControllerRef::You));
                }
                other => panic!("expected the typed host filter, got {other:?}"),
            }
        }
        other => panic!("expected `then attach it` = Attach, got {other:?}"),
    }

    assert!(
        !has_swallowed(PLAY_BLITZBALL, "Sidequest: Play Blitzball", "Condition_If"),
        "Condition_If must clear once the intervening-if attaches"
    );
    assert!(
        !has_swallowed(
            PLAY_BLITZBALL,
            "Sidequest: Play Blitzball",
            "Duration_ThisTurn"
        ),
        "Duration_ThisTurn must clear: the `DamageDealtThisTurn` quantity is the \
         typed evidence the detector's unit probe reads"
    );
}

// ===========================================================================
// P2 — Sidequest: Hunt the Mark (parse fidelity)
// ===========================================================================
//
// V5: the End trigger carries the exact opponent-control dies condition, both
// swallows clear, and the "Then if you control three or more Treasures"
// sub-ability survives (the preservation half of the row).

#[test]
fn hunt_the_mark_parse_carries_opponent_control_dies_condition() {
    let parsed = parse_card(HUNT_THE_MARK, "Sidequest: Hunt the Mark");
    let trigger = parsed
        .triggers
        .iter()
        .find(|t| t.mode == TriggerMode::Phase && t.phase == Some(Phase::End))
        .expect("Hunt the Mark must carry an End phase trigger");

    let condition = trigger
        .condition
        .as_ref()
        .expect("the end-step intervening-if must attach to the trigger");
    match condition {
        TriggerCondition::QuantityComparison {
            lhs:
                QuantityExpr::Ref {
                    qty:
                        QuantityRef::ZoneChangeCountThisTurn {
                            from: Some(Zone::Battlefield),
                            to: Some(Zone::Graveyard),
                            filter,
                        },
                },
            comparator: Comparator::GE,
            rhs: QuantityExpr::Fixed { value: 1 },
        } => {
            let TargetFilter::Typed(tf) = filter else {
                panic!("expected a typed creature filter, got {filter:?}");
            };
            assert!(
                tf.type_filters.contains(&TypeFilter::Creature),
                "expected the Creature type filter, got {:?}",
                tf.type_filters
            );
            assert_eq!(
                tf.controller,
                Some(ControllerRef::Opponent),
                "CR 608.2h: \"under an opponent's control\" is the dead permanent's \
                 last-known controller"
            );
            assert!(
                tf.properties.iter().any(|prop| matches!(
                    prop,
                    FilterProp::InZone {
                        zone: Zone::Battlefield
                    }
                )),
                "CR 109.4: the dies condition is battlefield-scoped, got {:?}",
                tf.properties
            );
        }
        other => panic!("expected the opponent-control zone-change count, got {other:?}"),
    }
    assert_eq!(
        trigger.constraint,
        Some(TriggerConstraint::OnlyDuringYourTurn)
    );

    // Reach-guard for the swallow negatives: execute is the Treasure token, and
    // its conditional sibling (the 3+ Treasures transform) survived.
    let execute = trigger
        .execute
        .as_ref()
        .expect("the End trigger has an execute body");
    assert!(
        matches!(&*execute.effect, Effect::Token { .. }),
        "execute must be the Treasure token effect, not Unimplemented: {:?}",
        execute.effect
    );
    let sub = execute
        .sub_ability
        .as_ref()
        .expect("the \"Then if you control three or more Treasures\" sub-ability survives");
    assert!(
        matches!(&*sub.effect, Effect::Transform { .. }),
        "the conditional sub-ability must stay the transform: {:?}",
        sub.effect
    );

    assert!(
        !has_swallowed(HUNT_THE_MARK, "Sidequest: Hunt the Mark", "Condition_If"),
        "Condition_If must clear once the intervening-if attaches"
    );
    assert!(
        !has_swallowed(
            HUNT_THE_MARK,
            "Sidequest: Hunt the Mark",
            "Duration_ThisTurn"
        ),
        "Duration_ThisTurn must clear: the `ZoneChangeCountThisTurn` quantity is \
         the typed evidence the detector's unit probe reads"
    );
}

// ---------------------------------------------------------------------------
// Runtime harness
// ---------------------------------------------------------------------------

/// Drive interactive windows until `stop` holds. `targets` answers
/// declared-target prompts in FIFO order (CR 601.2c declaration order for
/// spells, CR 603.3d for triggers — the engine surfaces the trigger variant as
/// `WaitingFor::TriggerTargetSelection`). The one turn-based declaration the
/// helper answers is the active player's attack declaration: CR 508.1a lets the
/// active player choose which creatures, IF ANY, attack, so a row whose board
/// holds a legal attacker but whose scenario is a no-attack row submits the
/// empty declaration explicitly — a deliberate play, not a silent skip (the
/// prompt only surfaces when a legal attacker exists, and attacking rows drive
/// through `run_combat` instead). Every other window panics: a silent skip must
/// never make a negative row pass vacuously. Mirrors `rules.rs`'s drive loop
/// (98) and the `drain_order_triggers_with_identity` idiom.
fn drive(
    runner: &mut GameRunner,
    targets: &mut Vec<TargetRef>,
    mut stop: impl FnMut(&GameRunner) -> bool,
) {
    for _ in 0..120 {
        if stop(runner) {
            return;
        }
        let action = match runner.state().waiting_for.clone() {
            WaitingFor::OrderTriggers { triggers, .. } => GameAction::OrderTriggers {
                order: (0..triggers.len()).collect(),
            },
            WaitingFor::TargetSelection { .. } | WaitingFor::TriggerTargetSelection { .. } => {
                if targets.is_empty() {
                    panic!(
                        "drive: a target prompt arrived with an empty queue: {:?}",
                        runner.state().waiting_for
                    );
                }
                GameAction::ChooseTarget {
                    target: Some(targets.remove(0)),
                }
            }
            // CR 508.1a: "chooses which creatures that they control, if any,
            // will attack" — no-attack rows declare that choice here.
            WaitingFor::DeclareAttackers { .. } => GameAction::DeclareAttackers {
                attacks: vec![],
                bands: vec![],
            },
            WaitingFor::Priority { .. } => GameAction::PassPriority,
            other => panic!("drive: unexpected window {other:?}"),
        };
        runner
            .act(action)
            .unwrap_or_else(|err| panic!("drive: action rejected: {err:?}"));
    }
    panic!("drive: the stop condition was not reached within the iteration budget");
}

/// A quiet priority window in `phase` — the stop shape every runtime row uses.
fn at_phase_quiet(runner: &GameRunner, phase: Phase) -> bool {
    runner.state().phase == phase
        && runner.state().stack.is_empty()
        && matches!(runner.state().waiting_for, WaitingFor::Priority { .. })
}

/// Inject a real Equipment back face onto `target` from `donor` — the
/// `cr733_resolved_transform.rs` recipe. CR 701.27a: only a permanent with a
/// back face can transform, and CR 301.5 makes the transformed back face (an
/// Equipment) the attachable object, so without this the attach could never be
/// observed at all.
fn inject_back_face(runner: &mut GameRunner, target: ObjectId, donor: ObjectId) {
    let back_face = snapshot_object_face(&runner.state().objects[&donor]);
    runner
        .state_mut()
        .objects
        .get_mut(&target)
        .expect("the double-faced permanent exists")
        .back_face = Some(back_face);
}

/// Token Treasures named "Treasure" controlled by `player` (the
/// `issue_3876_gadrak_treasure_count.rs` idiom).
fn treasure_token_count(runner: &GameRunner, player: PlayerId) -> usize {
    runner
        .state()
        .objects
        .values()
        .filter(|o| {
            o.controller == player
                && o.zone == Zone::Battlefield
                && o.is_token
                && o.name == "Treasure"
        })
        .count()
}

/// Treasures counted by the SUBTYPE. R6 must use this: its pre-seeded Treasures
/// are non-token artifacts, so the token-filtered count above would report 1 of
/// 3 and the conditional transform could never be observed.
fn treasure_subtype_count(runner: &GameRunner, player: PlayerId) -> usize {
    runner
        .state()
        .objects
        .values()
        .filter(|o| {
            o.controller == player
                && o.zone == Zone::Battlefield
                && o.card_types.subtypes.iter().any(|s| s == "Treasure")
        })
        .count()
}

// ===========================================================================
// R1–R3 — Sidequest: Play Blitzball (real trigger pipeline)
// ===========================================================================

/// Play Blitzball board: the enchantment, one attacker (also the sole legal
/// attach host), and a donor Equipment back face on P1. Returns
/// (runner, sidequest, attacker).
fn blitzball_board(attacker_power: i32) -> (GameRunner, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let sidequest = scenario
        .add_enchantment_from_oracle(P0, "Sidequest: Play Blitzball", PLAY_BLITZBALL)
        .id();
    let attacker = scenario
        .add_creature(P0, "Grizzly Bears", attacker_power, 4)
        .id();
    let donor = scenario
        .add_artifact_from_oracle(P1, "World Champion, Celestial Weapon", WORLD_CHAMPION)
        .with_subtypes(vec!["Equipment"])
        .id();
    let mut runner = scenario.build();
    inject_back_face(&mut runner, sidequest, donor);
    (runner, sidequest, attacker)
}

/// R1: 6 combat damage at end of combat → the trigger fires, transforms, AND
/// attaches the transformed source to the chosen host.
///
/// The attacker is a 4/4: the card's OWN beginning-of-combat ability pumps it
/// to 6/4, so the damage the condition reads is the post-pump 6 (the reach-guard
/// below proves the pump landed — an un-pumped 4 would fail the life assertion).
///
/// Revert-failing: under a U1 revert the condition never attaches and the
/// trigger's clause tree collapses (no transform); under a U3 revert the
/// attachment stays `ParentTarget` and resolves to a self-attach no-op, so
/// `attached_to` stays `None` and the host's attachment list stays empty.
#[test]
fn play_blitzball_six_combat_damage_transforms_and_attaches_to_host() {
    let (mut runner, sidequest, attacker) = blitzball_board(4);

    // Resolve the beginning-of-combat Pump trigger (full card text, so this
    // prompt exists and must be answered — unanswered it would strand the run).
    let mut pump_targets = vec![TargetRef::Object(attacker)];
    drive(&mut runner, &mut pump_targets, |r| {
        at_phase_quiet(r, Phase::BeginCombat)
    });

    run_combat(&mut runner, vec![attacker], vec![]);

    // End of combat: the trigger fires (condition true) and announces its
    // attach host. The attacker is the sole creature and thus the only legal
    // host; the same object also receives the pump, so this queue is one slot.
    let mut attach_targets = vec![TargetRef::Object(attacker)];
    drive(&mut runner, &mut attach_targets, |r| {
        at_phase_quiet(r, Phase::EndCombat)
    });

    // Reach-guards: the 6 damage actually happened and the window was reached,
    // so the transform/attach assertions below are not vacuous.
    assert_eq!(
        runner.state().players[P1.0 as usize].life,
        14,
        "reach-guard: the 4/4 attacker was pumped to 6/4 by the card's own \
         beginning-of-combat ability and dealt 6 combat damage to P1"
    );
    assert_eq!(runner.state().phase, Phase::EndCombat);

    assert!(
        runner.state().objects[&sidequest].transformed,
        "CR 120.2a + CR 603.4: 6 combat damage satisfies the intervening-if, so \
         the transform instruction resolves"
    );
    assert_eq!(
        runner.state().objects[&sidequest].attached_to,
        Some(AttachTarget::Object(attacker)),
        "CR 608.2c + CR 701.3a: \"then attach it\" attaches the SOURCE to the \
         chosen host"
    );
    assert!(
        runner.state().objects[&attacker]
            .attachments
            .contains(&sidequest),
        "the host must list the transformed Sidequest among its attachments"
    );
}

/// R2: 5 combat damage (< 6) → the intervening-if is false at fire time → NO
/// transform and nothing attached. The attacker is a 3/3, so the card's own
/// beginning-of-combat pump takes it to 5/3 (still below the threshold — a
/// 5-power base would be pumped to 7 and wrongly fire). Reach-guards: the
/// damage happened (life 15) and the EndCombat window was reached.
#[test]
fn play_blitzball_five_combat_damage_does_not_fire() {
    let (mut runner, sidequest, attacker) = blitzball_board(3);

    let mut pump_targets = vec![TargetRef::Object(attacker)];
    drive(&mut runner, &mut pump_targets, |r| {
        at_phase_quiet(r, Phase::BeginCombat)
    });

    run_combat(&mut runner, vec![attacker], vec![]);

    let mut attach_targets = vec![TargetRef::Object(attacker)];
    drive(&mut runner, &mut attach_targets, |r| {
        at_phase_quiet(r, Phase::EndCombat)
    });

    assert_eq!(
        runner.state().players[P1.0 as usize].life,
        15,
        "reach-guard: 5 combat damage was dealt (CR 120.2a)"
    );
    assert_eq!(runner.state().phase, Phase::EndCombat);
    assert!(
        !runner.state().objects[&sidequest].transformed,
        "5 < 6 → the intervening-if (CR 603.4) blocks the trigger at fire time"
    );
    assert_eq!(
        runner.state().objects[&sidequest].attached_to,
        None,
        "nothing was attached (the trigger never resolved)"
    );
    assert!(
        !runner.state().objects[&attacker]
            .attachments
            .contains(&sidequest),
        "the host's attachment list must stay empty"
    );
}

/// R3: 6 NONCOMBAT damage, 0 combat damage → NO transform. This is the
/// damage-kind discriminator: P1 sits at 14, so a threshold-only reading
/// (`DamageKindFilter::Any`) would satisfy "6 or more damage" and fire the
/// trigger — and because the row keeps a legal attach host on the battlefield
/// (a 1/1 that stays home), that wrong-kind trigger would resolve the transform
/// and the assertion below would flip. This row previously left the board
/// empty, so a wrong-kind regression would ALSO have left `!transformed` green
/// (the trigger would have been dropped for the missing host) — the negative
/// was not discriminating.
#[test]
fn play_blitzball_six_noncombat_damage_does_not_fire() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let sidequest = scenario
        .add_enchantment_from_oracle(P0, "Sidequest: Play Blitzball", PLAY_BLITZBALL)
        .id();
    // A legal attach host that never attacks: its presence is what makes the
    // wrong-kind regression observable (the trigger would otherwise be dropped
    // for the missing host even if its damage-kind condition wrongly matched).
    let host = scenario.add_creature(P0, "Homebody", 1, 1).id();
    let donor = scenario
        .add_artifact_from_oracle(P1, "World Champion, Celestial Weapon", WORLD_CHAMPION)
        .with_subtypes(vec!["Equipment"])
        .id();
    let bolt = scenario
        .add_spell_to_hand_from_oracle(P0, "Six Damage", true, "Deal 6 damage to target player.")
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let mut runner = scenario.build();
    inject_back_face(&mut runner, sidequest, donor);

    runner.cast(bolt).target_player(P1).resolve();

    // The 1/1 host is a legal target for the beginning-of-combat pump, so that
    // prompt exists and is answered first; the host then stays home (the drive
    // submits the empty attack declaration at CR 508.1a). The SECOND queued
    // target is the attach host: with the correct combat-only condition no
    // trigger fires and it goes unused, but a wrong-kind (`Any`) regression
    // would fire the trigger, demand its stack-time attach host, and resolve a
    // transform onto that host — failing the `!transformed` assertion below.
    let mut targets = vec![TargetRef::Object(host), TargetRef::Object(host)];
    drive(&mut runner, &mut targets, |r| {
        at_phase_quiet(r, Phase::EndCombat)
    });

    assert_eq!(
        runner.state().players[P1.0 as usize].life,
        14,
        "reach-guard: 6 noncombat damage WAS dealt — only the kind excludes it"
    );
    assert_eq!(runner.state().phase, Phase::EndCombat);
    assert_eq!(
        runner.state().objects[&host].zone,
        Zone::Battlefield,
        "reach-guard: a legal attach host exists at the end-of-combat window"
    );
    assert!(
        !runner.state().objects[&host].tapped,
        "reach-guard: the host stayed home (did not attack)"
    );
    assert!(
        !runner.state().objects[&sidequest].transformed,
        "CR 120.2a: 6 noncombat damage does not satisfy \"6 or more combat \
         damage\" (the kind axis discriminates, not the threshold); a wrong-kind \
         regression would transform onto the surviving host and fail here"
    );
}

// ===========================================================================
// R4–R6 — Sidequest: Hunt the Mark (real trigger pipeline)
// ===========================================================================

/// Hunt the Mark board: the enchantment, a victim controlled by `victim_owner`
/// (destroyed from hand with a {0} instant in PreCombatMain), a donor Equipment
/// back face, and optional pre-seeded non-token Treasures. Returns
/// (runner, sidequest, victim).
///
/// The donor supplies the back face only to make `back_face.is_some()` true, so
/// the "Then if you control three or more Treasures" transform is a LIVE
/// possibility rather than a disabled one. The face's identity is irrelevant to
/// these rows (no assertion reads its characteristics); the card's real back
/// face is Yiazmat, Ultimate Mark.
fn hunt_the_mark_board(
    victim_owner: PlayerId,
    seeded_treasures: usize,
) -> (GameRunner, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let sidequest = scenario
        .add_enchantment_from_oracle(P0, "Sidequest: Hunt the Mark", HUNT_THE_MARK)
        .id();
    let victim = scenario.add_creature(victim_owner, "Victim", 2, 2).id();
    let donor = scenario
        .add_artifact_from_oracle(P1, "World Champion, Celestial Weapon", WORLD_CHAMPION)
        .with_subtypes(vec!["Equipment"])
        .id();
    for _ in 0..seeded_treasures {
        scenario
            .add_artifact_from_oracle(P0, "Treasure", TREASURE_TOKEN)
            .with_subtypes(vec!["Treasure"])
            .id();
    }
    let destroy = scenario
        .add_spell_to_hand_from_oracle(P0, "Murder", true, DESTROY)
        .with_mana_cost(ManaCost::generic(0))
        .id();
    let mut runner = scenario.build();
    inject_back_face(&mut runner, sidequest, donor);
    runner.cast(destroy).target_object(victim).resolve();
    (runner, sidequest, victim)
}

/// R4: an OPPONENT's creature dies this turn → the end-step trigger fires and
/// creates a Treasure. The victim is in P1's graveyard and `!transformed` (1
/// Treasure < 3) is asserted with the back face injected, so the negative is a
/// live possibility rather than a disabled transform.
#[test]
fn hunt_the_mark_opponent_creature_death_creates_treasure() {
    let (mut runner, sidequest, victim) = hunt_the_mark_board(P1, 0);

    let mut targets = Vec::new();
    drive(&mut runner, &mut targets, |r| at_phase_quiet(r, Phase::End));

    // Reach-guard: the death actually happened under P1's control.
    assert_eq!(
        runner.state().objects[&victim].zone,
        Zone::Graveyard,
        "reach-guard: the destroyed creature died (CR 700.4)"
    );
    assert_eq!(runner.state().objects[&victim].controller, P1);
    assert_eq!(
        treasure_token_count(&runner, P0),
        1,
        "CR 603.4 + CR 608.2h: a creature died under an opponent's control → one \
         Treasure token"
    );
    assert_eq!(
        runner.state().phase,
        Phase::End,
        "the row's window was the end step"
    );
    assert!(
        !runner.state().objects[&sidequest].transformed,
        "1 Treasure < 3 → the conditional transform does not resolve"
    );
}

/// R5: YOUR OWN creature dies the same turn via the identical destroy path →
/// the possessor axis is false and NO Treasure is created. This is the
/// possessor discriminator: it fails if the combinator injected `You` into the
/// condition's typing in a way that matches, and fails under a U2 revert (no
/// condition → the trigger fires on any death). Reach-guard: the creature is in
/// the graveyard.
#[test]
fn hunt_the_mark_own_creature_death_creates_no_treasure() {
    let (mut runner, _sidequest, victim) = hunt_the_mark_board(P0, 0);

    let mut targets = Vec::new();
    drive(&mut runner, &mut targets, |r| at_phase_quiet(r, Phase::End));

    assert_eq!(
        runner.state().objects[&victim].zone,
        Zone::Graveyard,
        "reach-guard: the creature DID die — only its controller differs"
    );
    assert_eq!(
        treasure_token_count(&runner, P0),
        0,
        "\"died under an opponent's control\" is false for your own creature"
    );
}

/// R6 (PRESERVATION row, explicitly not revert-failing by itself): two
/// pre-seeded Treasures + the token created by the trigger reach exactly three,
/// so the surviving "Then if you control three or more Treasures" sub-ability
/// resolves and transforms. This pins the non-regression of the conditional
/// clause that U2 must leave attached.
#[test]
fn hunt_the_mark_three_treasures_transform_preservation_row() {
    let (mut runner, sidequest, victim) = hunt_the_mark_board(P1, 2);

    let mut targets = Vec::new();
    drive(&mut runner, &mut targets, |r| at_phase_quiet(r, Phase::End));

    assert_eq!(
        runner.state().objects[&victim].zone,
        Zone::Graveyard,
        "reach-guard: the opponent's creature died"
    );
    assert_eq!(
        treasure_subtype_count(&runner, P0),
        3,
        "reach-guard: 2 seeded + 1 created = exactly 3 Treasures (counted by \
         subtype, since the seeded ones are non-token artifacts)"
    );
    assert!(
        runner.state().objects[&sidequest].transformed,
        "PRESERVATION row (not revert-failing by itself): the conditional \
         transform fires at 3 Treasures"
    );
}
