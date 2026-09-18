//! Wickerfolk Indomitable — graveyard cast permission with a COMPOSITE
//! additional cost: 2 life AND sacrificing an artifact or creature
//! (CR 601.2f + CR 118.8).
//!
//! "You may cast this card from your graveyard by paying 2 life and sacrificing
//! an artifact or creature in addition to paying its other costs."
//!
//! The permission keeps the spell's printed mana cost (CR 601.2f: an ADDITIONAL
//! cost, not an alternative one) and requires BOTH legs of the ` and `-joined
//! gerund rider (CR 118.8a: any number of additional costs may apply). These
//! tests drive the real cast pipeline through the scenario `GameRunner` /
//! `SpellCast` driver and assert both legs are actually paid.

use engine::game::casting::{can_cast_object_now, spell_objects_available_to_cast};
use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::mana::{ManaCost, ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const WICKERFOLK_ORACLE: &str = "You may cast this card from your graveyard by paying 2 life and sacrificing an artifact or creature in addition to paying its other costs.";

fn pool_units(colors: &[ManaType]) -> Vec<ManaUnit> {
    let dummy = engine::types::identifiers::ObjectId(0);
    colors
        .iter()
        .map(|&color| ManaUnit::new(color, dummy, false, vec![]))
        .collect()
}

/// Wickerfolk's printed cost is {3}{B}; the composite rider behavior under
/// test does not depend on the mana amount, so the stand-in uses {1} and a
/// single colorless unit to keep the pool minimal.
fn stage_wickerfolk(scenario: &mut GameScenario) -> engine::types::identifiers::ObjectId {
    scenario
        .add_creature_to_graveyard(P0, "Wickerfolk Indomitable", 4, 3)
        .with_mana_cost(ManaCost::generic(1))
        .from_oracle_text(WICKERFOLK_ORACLE)
        .id()
}

/// CR 601.2f + CR 118.8 + CR 119.4 + CR 701.21a: end-to-end — casting
/// Wickerfolk from the graveyard pays its {1} AND 2 life AND sacrifices the
/// artifact. DISCRIMINATING: before per-component de-conjugation the sacrifice
/// leg stayed `Unimplemented`, so the permission paid only life — the fodder
/// would stay on the battlefield and the cast would be rejected at payment.
#[test]
fn wickerfolk_graveyard_cast_pays_life_and_sacrifices() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = stage_wickerfolk(&mut scenario);
    let fodder = scenario
        .add_creature(P0, "Fodder Artifact", 0, 0)
        .as_artifact()
        .id();
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let mut runner = scenario.build();

    assert!(
        spell_objects_available_to_cast(runner.state(), P0).contains(&wickerfolk),
        "the graveyard permission must surface Wickerfolk as castable"
    );
    let outcome = runner.cast(wickerfolk).sacrifice_with(&[fodder]).resolve();

    outcome.assert_life_delta(P0, -2);
    outcome.assert_zone(&[fodder], Zone::Graveyard);
    outcome.assert_zone(&[wickerfolk], Zone::Battlefield);
}

/// CR 601.2h + CR 118.3: the sacrifice leg is a real cost gate — with no
/// artifact or creature P0 controls, the composite additional cost is
/// unpayable and the graveyard cast must not be offered. Reach-guard: the same
/// setup with a controlled fodder IS castable, so the block is affordability-
/// specific, not a blanket refusal.
#[test]
fn wickerfolk_graveyard_cast_blocked_without_artifact_or_creature() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = stage_wickerfolk(&mut scenario);
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let runner = scenario.build();

    assert!(
        !can_cast_object_now(runner.state(), P0, wickerfolk),
        "with no artifact or creature to sacrifice the composite additional cost \
         (CR 601.2h) is unpayable, so the graveyard cast must not be offered"
    );

    let mut reachable = GameScenario::new();
    reachable.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = stage_wickerfolk(&mut reachable);
    reachable.add_creature(P0, "Fodder", 1, 1);
    reachable.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let runner = reachable.build();
    assert!(
        can_cast_object_now(runner.state(), P0, wickerfolk),
        "reach-guard: a fodder P0 controls makes the same cast legal"
    );
}

/// CR 701.21a: only permanents the caster controls can be sacrificed — an
/// opponent's creature is NOT a legal payment. Reach-guard: the same creature
/// under P0's control makes the cast legal, so the block is control-specific.
#[test]
fn wickerfolk_graveyard_cast_requires_controlling_the_sacrifice() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = stage_wickerfolk(&mut scenario);
    scenario.add_creature(P1, "Opponent Fodder", 1, 1);
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let runner = scenario.build();

    assert!(
        !can_cast_object_now(runner.state(), P0, wickerfolk),
        "an opponent's creature is not a legal sacrifice for P0 (CR 701.21a), so \
         the graveyard cast must not be offered"
    );

    let mut reachable = GameScenario::new();
    reachable.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = stage_wickerfolk(&mut reachable);
    reachable.add_creature(P0, "Own Fodder", 1, 1);
    reachable.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let runner = reachable.build();
    assert!(
        can_cast_object_now(runner.state(), P0, wickerfolk),
        "reach-guard: the same creature under P0's control is a legal sacrifice"
    );
}

/// CR 601.2b: the composite additional cost belongs to the graveyard
/// permission only — a normal hand cast of the same card loses no life and
/// sacrifices nothing. This is the control for the end-to-end test above.
#[test]
fn wickerfolk_hand_cast_pays_no_life() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let wickerfolk = scenario
        .add_creature_to_hand(P0, "Wickerfolk Indomitable", 4, 3)
        .with_mana_cost(ManaCost::generic(1))
        .from_oracle_text(WICKERFOLK_ORACLE)
        .id();
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Colorless]));
    let mut runner = scenario.build();

    let outcome = runner.cast(wickerfolk).resolve();

    outcome.assert_life_delta(P0, 0);
    outcome.assert_zone(&[wickerfolk], Zone::Battlefield);
}
