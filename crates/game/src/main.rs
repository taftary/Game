//! `game` — playable binary: player walk + cameras + chunk streaming.
//!
//! v1 minimum (see `plans/player-sphere-movement`): a headless demo loop at
//! the 20 Hz sim rate. A scripted walk moves the player across the sphere,
//! the camera cycles through every mode, and chunks stream with the same
//! [`visible_hemisphere`](game_engine::render::visible_hemisphere) rule the
//! flat map uses today — loading ahead of movement, unloading the trail
//! after the grace period. Interactive `winit` input binds later; it will
//! feed the same [`MoveInput`](game::player::MoveInput).

use game::camera::{CameraMode, PlayerCamera};
use game::hud::{Hud, HudInputs};
use game::journey::{Journey, JourneyEvent};
use game::player::{MoveInput, Player};
use game::save::Autosave;
use game::streaming::ChunkStreamer;
use game_engine::catalog::format::CATALOG_VERSION_SYNTH;
use game_engine::flight::{FlyToExec, ShipState, Target, plan_fly_to};
use game_engine::frames::{BodyId, FrameChain, FrameId, FrameLink, TransitionReason};
use game_engine::handoff::HandoffMonitor;
use game_engine::hexsphere::HexSphere;
use game_engine::render::{project_to_tangent, visible_hemisphere};
use game_engine::save::{AutosaveRing, SaveEnvelope, SaveMetadata, decode, encode};
use game_engine::time::CompressionClock;
use glam::{DQuat, DVec3};
use std::time::{SystemTime, UNIX_EPOCH};

/// Demo planet radius in meters.
const RADIUS: f32 = 100.0;
/// HexSphere subdivisions for the demo (162 cells — cheap, same rules).
const SUBDIVISIONS: u32 = 2;
/// Full-deflection walk speed in meters per second.
const WALK_SPEED: f32 = 8.0;
/// Fixed sim step in seconds (20 Hz, per `docs/techstack/simulation.md`).
const DT: f32 = 0.05;
/// Ticks an unseen chunk survives before eviction.
const UNLOAD_GRACE_TICKS: u64 = 10;

/// One scripted leg: heading-relative walk input, step count, camera
/// mode for the leg. Thrust walks along the heading, turn rotates it.
const LEGS: [(MoveInput, u64, CameraMode); 4] = [
    (
        MoveInput {
            forward: 1.0,
            turn: 0.0,
        },
        12,
        CameraMode::Follow,
    ),
    (
        MoveInput {
            forward: 1.0,
            turn: 0.5,
        },
        12,
        CameraMode::FirstPerson,
    ),
    (
        MoveInput {
            forward: 0.0,
            turn: -1.0,
        },
        12,
        CameraMode::ThirdPerson,
    ),
    (
        MoveInput {
            forward: 0.0,
            turn: 0.0,
        },
        12,
        CameraMode::Global,
    ),
];

fn main() {
    let interval = parse_interval_arg();
    run_player_demo();
    run_journey_trace();
    run_hud_trace();
    run_autosave_trace(interval);
}

/// `--autosave-interval SECONDS` (default 120, clamped 30–600 by
/// `game::save`). Everything else is a usage error.
fn parse_interval_arg() -> f64 {
    let mut interval = game::save::DEFAULT_INTERVAL_S;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--autosave-interval" => match args.next().and_then(|v| v.parse::<f64>().ok()) {
                Some(v) => interval = v,
                None => {
                    eprintln!("usage: game [--autosave-interval SECONDS]");
                    std::process::exit(2);
                }
            },
            "--help" | "-h" => {
                println!("usage: game [--autosave-interval SECONDS]");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other:?}\nusage: game [--autosave-interval SECONDS]");
                std::process::exit(2);
            }
        }
    }
    interval
}

fn run_player_demo() {
    let mesh = HexSphere::generate(SUBDIVISIONS, RADIUS);
    let mut player = Player::new(0.0, 0.0, RADIUS, WALK_SPEED);
    let mut camera = PlayerCamera::new(RADIUS);
    let mut streamer = ChunkStreamer::new(UNLOAD_GRACE_TICKS);

    println!(
        "player-demo: {} cells, R={RADIUS}m, {} ticks @ {DT}s",
        mesh.cell_count(),
        LEGS.iter().map(|leg| leg.1).sum::<u64>(),
    );

    let mut tick = 0_u64;
    for (input, steps, mode) in LEGS {
        camera.set_mode(mode);
        for _ in 0..steps {
            tick += 1;
            player.update(input, DT);
            let position = player.position().to_array();
            // Same rule as the flat map today: the player's hemisphere,
            // recentered on the player every tick.
            let desired = visible_hemisphere(&mesh, position);
            let delta = streamer.update(&desired, tick);
            // The player is the flat-map viewpoint, so it projects to origin.
            let flat_self = project_to_tangent(position, position);
            debug_assert!(
                flat_self[0].abs() < 1e-4 && flat_self[1].abs() < 1e-4,
                "flat map must recenter on the player, got {flat_self:?}"
            );
            let eye = camera.eye(&player);
            println!(
                "t={tick:02} mode={mode:?} lon={:7.2} lat={:6.2} hdg={:6.1} want={} +{} -{} have={} eye=({:.1},{:.1},{:.1})",
                player.longitude().to_degrees(),
                player.latitude().to_degrees(),
                player.heading().to_degrees(),
                desired.len(),
                delta.loaded.len(),
                delta.unloaded.len(),
                streamer.loaded_count(),
                eye.x,
                eye.y,
                eye.z,
            );
        }
    }

    println!(
        "done: lon={:.2} lat={:.2} hdg={:.1} loaded={}",
        player.longitude().to_degrees(),
        player.latitude().to_degrees(),
        player.heading().to_degrees(),
        streamer.loaded_count(),
    );
}

fn run_journey_trace() {
    // Journey top segment (UMAP-009): scripted GalaxyMap → SystemMap →
    // Orbit → back traversal. Same event script the lib regression pins;
    // the headless gate replays it and prints the state hashes.
    let mut journey = Journey::new(99);
    println!(
        "journey start: {:?} hash={:016x}",
        journey.state(),
        journey.state_hash()
    );
    for event in [
        JourneyEvent::SelectStar(2),
        JourneyEvent::EnterSystem,
        JourneyEvent::SelectPlanet(1),
        JourneyEvent::FocusPlanet(Some(1)),
        JourneyEvent::FocusPlanet(None),
        JourneyEvent::EnterOrbit,
        JourneyEvent::Ascend,
        JourneyEvent::Ascend,
    ] {
        let effects = journey.update(event);
        println!(
            "journey {event:?} -> {:?} hash={:016x} fx={effects:?}",
            journey.active_layer(),
            journey.state_hash(),
        );
    }
}

fn run_hud_trace() {
    let mut ship = ShipState {
        chain: FrameChain::new(
            FrameId::SolarSystem,
            DVec3::new(2.0, 0.0, 0.0),
            DQuat::IDENTITY,
            vec![FrameLink::identity(); 4],
        ),
        vel: DVec3::ZERO,
        mass_kg: 5_000.0,
        fuel: 1.0,
    };
    let mut clock = CompressionClock::default();
    let target = Target::new(FrameId::SolarSystem, DVec3::new(3.0, 0.0, 0.0))
        .expect("trace target is finite");
    let plan = plan_fly_to(FrameId::SolarSystem, ship.chain.position(), &target, 0.0)
        .expect("trace target is outside arrival sphere");
    let mut executor = FlyToExec::new(plan);
    executor.commit().expect("trace fly-to commits once");
    let mut monitor = HandoffMonitor::new(BodyId::EARTH);
    let mut hud = Hud::new();

    println!("hud-trace: begin");
    for (step, weight) in [0.0, 0.25, 0.5, 1.0].into_iter().enumerate() {
        if step == 2 {
            clock.slew(100.0, 1.0);
        }
        monitor.update(weight);
        let events = monitor.events();
        let frame = hud.update(&HudInputs {
            ship: &ship,
            clock: &clock,
            executor: Some(&executor),
            target_label: Some("Earth orbit"),
            soi_weight: weight,
            soi_events: &events,
            body_label: Some("earth"),
        });
        println!(
            "hud t={step} {} | {} | soi={} | target={}",
            frame.frame_line,
            frame.time_line,
            frame
                .soi
                .as_ref()
                .map(|soi| soi.message.as_str())
                .unwrap_or("hidden"),
            frame
                .target
                .as_ref()
                .map(|target| target.line.as_str())
                .unwrap_or("hidden")
        );
        ship.chain
            .set_position(ship.chain.position() + DVec3::new(0.01, 0.0, 0.0));
    }
    println!("hud-trace: end");
}

/// ADR-004 autosave evidence: every trigger fires, a truncated save is
/// rejected, the recovery snapshot loads, and resume is state-identical.
fn run_autosave_trace(interval_s: f64) {
    let mut ship = ShipState {
        chain: FrameChain::new(
            FrameId::SolarSystem,
            DVec3::new(2.0, 0.0, 0.0),
            DQuat::IDENTITY,
            vec![FrameLink::identity(); 4],
        ),
        vel: DVec3::new(0.0, 0.1, 0.0),
        mass_kg: 5_000.0,
        fuel: f64::INFINITY,
    };
    let mut clock = CompressionClock::default();
    clock.slew(100.0, 1.0);
    let target = Target::new(FrameId::SolarSystem, DVec3::new(5.0, 0.0, 0.0))
        .expect("trace target is finite");
    let plan = plan_fly_to(FrameId::SolarSystem, ship.chain.position(), &target, 0.0)
        .expect("trace target is outside arrival sphere");
    let mut executor = FlyToExec::new(plan);
    let mut monitor = HandoffMonitor::new(BodyId::EARTH);
    let ring = AutosaveRing::new("saves", 3);
    let mut autosave = Autosave::new(interval_s);
    let mut saved: Vec<(game::save::SaveReason, game_engine::flight::ShipSnapshot)> = Vec::new();
    let mut sim = 0.0_f64;
    let step =
        |reason: game::save::SaveReason,
         ship: &ShipState,
         plan: Option<game_engine::flight::FlyToPlan>,
         clock: &CompressionClock,
         autosave: &Autosave,
         sim_time_s: f64,
         saved: &mut Vec<(game::save::SaveReason, game_engine::flight::ShipSnapshot)>| {
            let snapshot = game_engine::flight::ShipSnapshot::capture(ship, plan, clock);
            let envelope = SaveEnvelope {
                metadata: SaveMetadata {
                    master_seed: 99,
                    real_unix_s: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0),
                    sim_time_s,
                    playtime_s: autosave.playtime_s(),
                    catalog_versions: vec![CATALOG_VERSION_SYNTH.to_owned()],
                },
                ship: snapshot.clone(),
            };
            let bytes = encode(&envelope).expect("demo envelope encodes");
            let path = ring.write(&bytes).expect("demo write succeeds");
            println!(
                "save: reason={} file={} bytes={}",
                reason.name(),
                path.display(),
                bytes.len()
            );
            saved.push((reason, snapshot));
        };

    println!("autosave-trace: begin interval={}", autosave.interval_s());
    // Periodic: one full interval of caller-fed dt.
    sim += autosave.interval_s();
    let reason = autosave
        .tick(autosave.interval_s(), sim)
        .expect("one full interval must fire periodic");
    step(reason, &ship, None, &clock, &autosave, sim, &mut saved);
    // Fly-to start (commit).
    executor.commit().expect("trace fly-to commits once");
    sim += 10.0;
    let reason = autosave.on_fly_to_start(sim);
    step(
        reason,
        &ship,
        Some(executor.plan()),
        &clock,
        &autosave,
        sim,
        &mut saved,
    );
    // SOI handoff completed (Entered).
    monitor.update(1.0);
    assert!(
        monitor
            .events()
            .iter()
            .any(|e| matches!(e, game_engine::handoff::HandoffEvent::Entered { .. })),
        "weight 1.0 must complete the handoff"
    );
    sim += 10.0;
    let reason = autosave.on_soi_handoff(sim);
    step(
        reason,
        &ship,
        Some(executor.plan()),
        &clock,
        &autosave,
        sim,
        &mut saved,
    );
    // Confirmed frame transition (SolarSystem → StellarNeighborhood).
    sim += 10.0;
    ship.commit_to_parent(TransitionReason::BoundaryCrossing, sim)
        .expect("solar -> neighborhood commit");
    let reason = autosave.on_frame_transition(sim);
    step(
        reason,
        &ship,
        Some(executor.plan()),
        &clock,
        &autosave,
        sim,
        &mut saved,
    );
    // Fly-to complete (scripted sim time never runs backward).
    let t_end = executor.plan().t_end_s();
    assert!(executor.is_complete(t_end), "end time must complete");
    sim = sim.max(t_end);
    let reason = autosave.on_fly_to_complete(sim);
    step(
        reason,
        &ship,
        Some(executor.plan()),
        &clock,
        &autosave,
        sim,
        &mut saved,
    );
    // Clean quit.
    sim += 10.0;
    let reason = autosave.on_quit(sim);
    step(
        reason,
        &ship,
        Some(executor.plan()),
        &clock,
        &autosave,
        sim,
        &mut saved,
    );
    for event in autosave.events() {
        println!("save-log: {}@{:.1}", event.reason.name(), event.sim_time_s);
    }
    // Corruption: truncate the newest slot (quit), rejection + fallback.
    let slot0 = std::path::Path::new("saves/autosave.0.bin");
    let mut corrupted = std::fs::read(slot0).expect("slot 0 exists");
    corrupted.truncate(10);
    std::fs::write(slot0, &corrupted).expect("rewrite truncated slot");
    assert!(
        decode(&corrupted).is_err(),
        "truncated save must reject via length/checksum"
    );
    println!("corrupt: truncated save rejected ok");
    let loaded = ring.load_latest().expect("recovery must succeed");
    println!(
        "recovery: slot={} quarantined={} ok",
        loaded.slot, loaded.quarantined
    );
    // Resume: newest valid is the fly-to-complete snapshot (slot 1).
    let (expected_reason, expected) = saved
        .iter()
        .rev()
        .nth(1)
        .expect("fly-to-complete snapshot saved");
    assert_eq!(expected_reason.name(), "fly-to-complete");
    assert_eq!(
        loaded.envelope.ship, *expected,
        "resumed state must be bit-identical"
    );
    println!(
        "resume: frame={} pos=({:.3},{:.3},{:.3}) plan={} state-identical ok",
        loaded.envelope.ship.frame.name(),
        loaded.envelope.ship.position.x,
        loaded.envelope.ship.position.y,
        loaded.envelope.ship.position.z,
        loaded.envelope.ship.plan.is_some()
    );
    println!("autosave-trace: end");
}
