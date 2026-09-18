//! Maestros Ascendancy — graveyard cast permission with an ADDITIONAL
//! sacrifice cost and a linked stack-exit destination replacement
//! (CR 601.2f + CR 118.8 + CR 614.1a + CR 607.1).
//!
//! "Once during each of your turns, you may cast an instant or sorcery spell
//! from your graveyard by sacrificing a creature in addition to paying its
//! other costs. If a spell cast this way would be put into your graveyard,
//! exile it instead."
//!
//! The trailing destination sentence is a CR 614.1a replacement of the
//! stack→graveyard event, linked back to the permission by "cast this way"
//! (CR 607.1). It must be peeled BEFORE the additional-cost rider parses, and
//! it rides only the ELECTED permission — never a global flag. These tests
//! drive the real cast pipeline through the scenario `GameRunner` / `SpellCast`
//! driver and assert the resolved spell is exiled while a hand cast (no
//! permission involved) still goes to the graveyard.

use engine::game::casting::can_cast_object_now;
use engine::game::scenario::{GameScenario, P0};
use engine::types::mana::{ManaCost, ManaCostShard, ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::statics::StaticMode;
use engine::types::zones::Zone;

const MAESTROS_ORACLE: &str = "Once during each of your turns, you may cast an instant or sorcery spell from your graveyard by sacrificing a creature in addition to paying its other costs. If a spell cast this way would be put into your graveyard, exile it instead.";

const KARADOR_ORACLE: &str = "This spell costs {1} less to cast for each creature card in your graveyard.\nOnce during each of your turns, you may cast a creature spell from your graveyard.";

fn pool_units(colors: &[ManaType]) -> Vec<ManaUnit> {
    let dummy = engine::types::identifiers::ObjectId(0);
    colors
        .iter()
        .map(|&color| ManaUnit::new(color, dummy, false, vec![]))
        .collect()
}

fn stage_maestros(scenario: &mut GameScenario) -> engine::types::identifiers::ObjectId {
    scenario
        .add_creature(P0, "Maestros Ascendancy", 0, 0)
        .as_enchantment()
        .from_oracle_text(MAESTROS_ORACLE)
        .id()
}

/// A {R} instant with a no-target effect, staged in P0's graveyard.
fn stage_grave_instant(scenario: &mut GameScenario) -> engine::types::identifiers::ObjectId {
    scenario
        .add_spell_to_graveyard(P0, "Grave Opt", true)
        .with_mana_cost(ManaCost::Cost {
            shards: vec![ManaCostShard::Red],
            generic: 0,
        })
        .from_oracle_text("You gain 2 life.")
        .id()
}

/// CR 601.2f + CR 118.8 + CR 701.21a + CR 614.1a + CR 607.1: end-to-end — the
/// graveyard instant is cast by sacrificing a creature, and on resolution the
/// elected permission's destination replacement exiles it instead of the
/// graveyard. DISCRIMINATING: reverting the destination peel makes the
/// additional-cost rider see the still-present sentence and decline, so the
/// permission never reaches the cast at all.
#[test]
fn maestros_graveyard_cast_sacrifices_and_exiles_resolved_spell() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    stage_maestros(&mut scenario);
    let fodder = scenario.add_creature(P0, "Fodder", 1, 1).id();
    let spell = stage_grave_instant(&mut scenario);
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Red]));
    let mut runner = scenario.build();

    assert!(
        can_cast_object_now(runner.state(), P0, spell),
        "reach-guard: the elected graveyard permission must surface the instant as castable"
    );
    let outcome = runner.cast(spell).sacrifice_with(&[fodder]).resolve();

    outcome.assert_zone(&[fodder], Zone::Graveyard);
    outcome.assert_zone(&[spell], Zone::Exile);
}

/// CR 601.2b: the destination replacement belongs to the graveyard permission
/// only — the same instant cast from hand (no permission involved) resolves to
/// the graveyard normally. This is the control for the end-to-end test above.
#[test]
fn maestros_hand_cast_resolves_to_graveyard() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    stage_maestros(&mut scenario);
    let spell = scenario
        .add_spell_to_hand(P0, "Hand Opt", true)
        .with_mana_cost(ManaCost::Cost {
            shards: vec![ManaCostShard::Red],
            generic: 0,
        })
        .from_oracle_text("You gain 2 life.")
        .id();
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Red]));
    let mut runner = scenario.build();

    let outcome = runner.cast(spell).resolve();

    outcome.assert_zone(&[spell], Zone::Graveyard);
}

/// CR 601.2h + CR 118.3 + CR 701.21a: the sacrifice is a real cost gate — with
/// no creature P0 controls, the additional cost is unpayable and the graveyard
/// cast must not be offered. Reach-guard: the same setup with a controlled
/// fodder IS castable, so the block is affordability-specific.
#[test]
fn maestros_graveyard_cast_blocked_without_creature() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    stage_maestros(&mut scenario);
    let spell = stage_grave_instant(&mut scenario);
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Red]));
    let runner = scenario.build();

    assert!(
        !can_cast_object_now(runner.state(), P0, spell),
        "with no creature to sacrifice the additional cost (CR 601.2h) is \
         unpayable, so the graveyard cast must not be offered"
    );

    let mut reachable = GameScenario::new();
    reachable.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    stage_maestros(&mut reachable);
    reachable.add_creature(P0, "Fodder", 1, 1);
    let spell = stage_grave_instant(&mut reachable);
    reachable.with_mana_pool(P0, pool_units(&[ManaType::Red]));
    let runner = reachable.build();
    assert!(
        can_cast_object_now(runner.state(), P0, spell),
        "reach-guard: a creature P0 controls makes the same cast legal"
    );
}

/// CR 607.1 + CR 614.1a: the destination replacement is owned by the ELECTED
/// permission, not a global flag. Karador, Ghost Chieftain also grants a
/// graveyard cast permission (creature-filter, no destination), but the
/// instant is only eligible under Maestros — so the exile comes from Maestros's
/// replacement and Karador's permission must keep no destination.
#[test]
fn maestros_elected_permission_owns_the_destination_replacement() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain).with_life(P0, 20);
    let karador = scenario
        .add_creature(P0, "Karador, Ghost Chieftain", 3, 4)
        .as_legendary()
        .with_subtypes(vec!["Spirit", "Centaur"])
        .from_oracle_text(KARADOR_ORACLE)
        .id();
    stage_maestros(&mut scenario);
    let fodder = scenario.add_creature(P0, "Fodder", 1, 1).id();
    let spell = stage_grave_instant(&mut scenario);
    scenario.with_mana_pool(P0, pool_units(&[ManaType::Red]));
    let mut runner = scenario.build();

    let outcome = runner.cast(spell).sacrifice_with(&[fodder]).resolve();

    outcome.assert_zone(&[fodder], Zone::Graveyard);
    outcome.assert_zone(&[spell], Zone::Exile);

    // Mirror: the non-matching permission keeps no destination replacement, so
    // the exile is attributable to the elected Maestros permission alone.
    let karador_destination_is_none = runner.state().objects[&karador]
        .static_definitions
        .as_slice()
        .iter()
        .find_map(|def| match &def.mode {
            StaticMode::GraveyardCastPermission {
                graveyard_destination_replacement,
                ..
            } => Some(graveyard_destination_replacement.is_none()),
            _ => None,
        });
    assert_eq!(
        karador_destination_is_none,
        Some(true),
        "Karador's creature-filter permission must exist and keep no destination replacement"
    );
}
