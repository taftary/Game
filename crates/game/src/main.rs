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
use game::journey::{Journey, JourneyEvent};
use game::player::{MoveInput, Player};
use game::streaming::ChunkStreamer;
use game_engine::hexsphere::HexSphere;
use game_engine::render::{project_to_tangent, visible_hemisphere};

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
