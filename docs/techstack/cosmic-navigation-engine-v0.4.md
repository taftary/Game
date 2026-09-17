# Continuous Multi-Scale Cosmic Navigation Engine

## Technical Specification — v0.4

**Stack:** Rust + Vulkano  
**Mode:** Real, free-flight player navigation with select-to-focus automated transitions; piloted craft with true physical dynamics (thrust, inertia, orbital mechanics)  
**Anchor:** Real Earth coordinates for the terminal facility; real astronomical data wherever it exists, hierarchical seeded procedural generation elsewhere

## 1. Scale Hierarchy

Ten named waypoints spanning approximately 26 orders of magnitude. Distances are the **decade gap** to the previous waypoint — this is what "constant time per order of magnitude" is computed against for scripted fly-to transitions (free flight has no fixed pacing; see §3).

| # | Waypoint | Scale (v0.4) | Decades from prior | Content |
|---:|---|---:|---:|---|
| 1 | Cosmic Web | 10^26^ m | — | Filaments, voids, procedural large-scale structure |
| 2 | Galactic Supercluster (Laniakea-analog) | 10^24^ m | 2 | Galaxy clusters, intergalactic nebular structure |
| 3 | Local Group | 10^23^ m | 1 | Milky Way, Andromeda, Triangulum, dwarf satellites |
| 4 | Milky Way Galaxy | 10^21^ m | 2 | Spiral arms, core, dust lanes |
| 5 | Solar Neighborhood | 10^17^ m | 4 | Real nearby stars (Gaia), proper motion |
| 6 | Solar System | 10^14^ m | 3 | Real planetary ephemerides; planetary, Kuiper-belt, and heliopause region |
| 7 | Planetary Surface / Earth | 10^6^ m | 8 | Atmosphere, oceans, continents |
| 8 | Regional Aerial | 10^3^ m | 3 | Terrain, facility siting |
| 9 | Facility Exterior | 10^1^ m | 2 | Ground-level structures |
| 10 | Facility Interior / Control Center | 10^0^ m | 1 | Console, room geometry |

**Total:** 26 decades, still matching 10^26^ → 10^0^ exactly. Distribution: 2 + 1 + 2 + 4 + 3 + 8 + 3 + 2 + 1 = 26 decades.

### v0.1 → v0.2 → v0.3 → v0.4 correction log

- **v0.1 → v0.2:** corrected waypoints 2–4 (supercluster, Milky Way, Solar Neighborhood) to match measured astronomical values.
- **v0.2 → v0.3:** added Local Group as named waypoint #3 at 10^23^ m; raised Solar System bound to 10^13^ m to match heliopause scale.
- **v0.3 → v0.4:** raised Solar System bound to 10^14^ m to fully contain the heliopause (nose, flanks, and tail).

### Local Group resolution

The Local Group is now a named, player-visible waypoint at 10^23^ m. It bridges the supercluster and Milky Way frames, providing a dedicated transition for the Milky Way, Andromeda, Triangulum, and dwarf satellites. The remaining 2-decade Local Group → Milky Way transition is appropriate for scripted logarithmic pacing.

## 2. Navigation Model

### Default state

Free flight is player-controlled with thrust, orientation, and real physical momentum.

### Select-to-focus

The player selects a region or object, and the engine computes an automated fly-to trajectory. Scripted "constant time per decade" pacing applies only to this transition, as an easing and duration function. It does not apply to free flight.

### Time compression

Physics runs at true real-time whenever the ship is within a gravitationally significant radius of another body, supporting precision piloting such as low orbit with an approximately 90-minute period. Time compresses smoothly when far from all bodies, where galactic rotation periods of approximately 225 million years would otherwise be meaningless to a human player.

Time compression must be an explicit state machine tied to reference-frame occupancy, not a global multiplier. This prevents sphere-of-influence handoffs from desynchronizing.

## 3. Coordinate & Precision Architecture

**Decision:** hierarchical nested reference frames, Celestia / SpaceEngine / KSP-style, rather than a single rebasing `f64` world space.

### Frame chain

```text
Comoving Cosmological Frame   (Mpc)   — cosmic web, superclusters
  └─ Galactocentric Frame     (kpc)   — Milky Way
       └─ Local-Group Frame   (Mpc)   — Milky Way, Andromeda, satellites
            └─ Stellar-Neighborhood Frame (pc/ly) — nearby real stars
                 └─ Solar-System Barycentric Frame (AU) — real ephemerides
                      └─ Planetocentric Frame (km, ECEF) — Earth
                           └─ Local Tangent-Plane ENU Frame (m) — the facility
```

The camera and ship's true position are represented by a composed transform chain, resolved only through the active frame and its immediate parent. It is not flattened into one global coordinate.

### GPU precision

Use floating-origin rendering. Every frame, recenter world space on the camera and upload only camera-relative `f32` positions to Vulkano. The CPU-side frame tree carries `f64`, or per-frame non-dimensionalized units, for hierarchy portions far from the camera.

### Sphere-of-influence handoff

Use a soft patched-conic handoff. Position is C⁰ continuous and velocity is approximately Cⁱ continuous: transform both position and velocity state vectors before the physics step that crosses a frame boundary, then blend acceleration contributions across a radial band centered on the nominal SOI boundary.

- Begin handoff eligibility when the candidate secondary body's acceleration is at least 10⁻^3^ of the primary body's acceleration.
- Use a 10% radial blend band around the nominal SOI radius: from 1.05 × `r_soi` outside the boundary to 0.95 × `r_soi` inside it.
- Blend the primary-frame and secondary-frame acceleration contributions with a smoothstep weighting function, not a linear ramp, to avoid introducing an acceleration derivative kink.
- Outside the band, use the primary-dominant representation; inside the band, use the secondary-dominant representation.
- The player must observe no position jump, velocity kick, camera discontinuity, or HUD flicker.

The nominal Laplace SOI radius is:

\[
r_{\mathrm{SOI}} = a \left(\frac{m}{M}\right)^{2/5}
\]

where `a` is the secondary's semi-major axis about the primary, `m` is the secondary mass, and `M` is the primary mass.

**Note on Laplace SOI vs Hill sphere:**  
The Laplace SOI is used for **patched-conic handoffs**. For **orbital stability analysis** (e.g., moon orbit limits, ring system boundaries), use the Hill sphere:

\[
r_{\mathrm{Hill}} = a \left(\frac{m}{3M}\right)^{1/3}
\]

The Hill sphere is typically ~0.5–0.7 × the Laplace SOI radius for planet–Sun systems.

## 4. Rendering

### Depth strategy

Use a log-depth buffer within a single draw pass, Outerra-style by writing `log(z)` in the vertex shader, to extend usable depth range. Do not treat this as sufficient for all 26 decades in one pass.

Use multi-pass depth compositing for the full range:

- Render distant scale bands depth-cleared first, skybox-like: cosmic web, galaxy, and distant stars.
- Render current near-field geometry afterward with tight near and far planes.
- Composite the results.
- Decide per pair of scales which layers can share a depth pass; do not attempt one global depth range.

### LOD and streaming

Real catalog data cannot be fully memory-resident. Gaia's roughly 1.8 billion stars require a HEALPix spatial index, with on-demand streaming keyed by camera position and active frame.

**HEALPix resolution table:**

| Order | nside | N pixels | Resolution | Recommended use |
|---:|---:|---:|---:|---|
| 12 | 4096 | 2.0 × 10⁸ | 51.5″ | Default for v0.4, balanced memory/quality |
| 13 | 8192 | 8.1 × 10⁸ | 25.8″ | High-density galactic plane, mid-tier devices |
| 14 | 16384 | 3.2 × 10⁹ | 12.9″ | Premium devices, close-up stellar views |

- Use HEALPix order 12–14 for all-sky catalog tiling; choose the concrete order per catalog-density tier and device memory class.
- Reserve a 2 GB active star-tile memory budget, including decoded attributes and GPU-ready buffers.
- Target less than 100 ms asynchronous tile-load latency after data is available locally; prioritize view-frustum tiles, then predicted travel direction, then camera distance.
- If a requested tile is unavailable after 200 ms, render a deterministic procedural fallback using tile metadata: preserve approximate source count and mean color, distribute temporary sources with a seeded golden-angle lattice, and replace them seamlessly when catalog data arrives.
- Catalog and fallback content must share stable object/tile identifiers so visual replacement never affects selection, focus targeting, or navigation state.

## 5. Physics Model per Scale

Uniform-fidelity N-body simulation across all scales is computationally impractical and unnecessary. Use the cheapest model that is visually and physically indistinguishable from correct at each scale.

| Scale | What moves | Model |
|---|---|---|
| Cosmic web / supercluster | Nothing in real time | Static procedural density field, no live gravity |
| Local Group / Milky Way | Background stars | Analytic rotation-curve potential: Miyamoto-Nagai disk plus NFW halo for bulk kinematics |
| Solar neighborhood | Real nearby stars | Real Gaia proper motions, linear extrapolation valid for ±1000 years from Gaia DR3 epoch 2016.0 |
| Solar system, background bodies | Planets, moons | Precomputed ephemeris: VSOP87 default (3000 BCE – 3000 CE); optional DE440-derived simplified elements for outer planets (Jupiter–Neptune) in high-precision mode (10000 BCE – 10000 CE) |
| Player-piloted craft | The ship | Closed-form Keplerian elements for the dominant two-body case; enter soft patched-conic blending once a secondary acceleration reaches 10⁻^3^ of primary acceleration |
| Procedurally generated bodies | Exoplanets, asteroids | Orbital elements drawn from seed plus real exoplanet population statistics |

For any numerically integrated regime, use a symplectic integrator such as velocity Verlet or leapfrog. This is required for long-session energy stability; naive Euler integration visibly drifts or decays orbits over time.

### Galactic potential formulas (Milky Way)

**Miyamoto-Nagai disk potential:**

\[
\Phi_{\text{disk}}(R, z) = -\frac{GM}{\sqrt{R^2 + \left(a + \sqrt{z^2 + b^2}\right)^2}}
\]

where:
- \( R \) = cylindrical radius in the galactic plane.
- \( z \) = vertical height above the plane.
- \( a \) = disk scale length (typical: ~6.5 kpc for Milky Way).
- \( b \) = disk scale height (typical: ~0.26 kpc for Milky Way).

**NFW halo potential:**

\[
\Phi_{\text{NFW}}(r) = -\frac{GM}{r} \ln\left(1 + \frac{r}{r_s}\right)
\]

where:
- \( r \) = spherical radius from galactic center.
- \( r_s \) = NFW scale radius (typical: ~20 kpc for Milky Way-like halos).

**Total potential:**

\[
\Phi_{\text{total}} = \Phi_{\text{disk}} + \Phi_{\text{NFW}}
\]

### Unit non-dimensionalization

Raw SI units, with `G = 6.674 × 10⁻^11^`, distances up to 10^26^ m, and masses near 10^42^ kg, are poorly conditioned for `f32` and `f64` force calculations. Each active frame uses a well-conditioned local unit system.

Examples:

- Solar-system frame: AU, day, solar masses; normalize `G` to approximately 1.
- Planetary frame: km, s, planet masses.

## 6. Procedural Generation & Seeding

**Decision:** use hierarchical seeds derived per region and scale from a single master seed. This enables independent regeneration and debugging of one region without changing others.

```text
region_seed = hash(master_seed, frame_id, cell_coordinates)
```

### Domain separation

Never reuse a raw region seed directly across content layers, as correlated artifacts can appear. Derive dedicated sub-seeds instead:

```text
hash(region_seed, "galaxy_arms")
hash(region_seed, "star_field")
hash(region_seed, "crater_field")
```

### Real-data override

Where a cataloged real object exists, such as a Gaia star or JPL-tracked planet, it is a fixed override. Suppress procedural generation inside an exclusion radius around it; perform that check before the procedural hash fires for the cell.

### Determinism

Procedural content is a pure function of `(seed, position)` and is never stored. Flying away and returning, or starting another session with the same seed, reproduces identical procedural regions with no save data required for that content.

## 7. Math Toolkit

| Technique | Use | Why |
|---|---|---|
| Quaternions | Camera and craft orientation across frame boundaries | Avoids gimbal lock and composes cleanly frame-to-frame |
| Complex numbers | Spiral galaxy arm placement: `z = r₀ · e^(i · b · θ)` | One complex exponential per point; matches logarithmic-spiral arm form |
| Fibonacci / golden-angle lattice, golden angle approximately 137.5^∘^ | Near-uniform distribution of N points: shell stars, cluster-node galaxies, surface craters | O(n), deterministic from seed plus index, no relaxation or Poisson-disk iteration, avoids clustering artifacts |
| Golden-ratio irrational rotation hash | Procedural chunk-boundary anti-tiling | Cheaply prevents visible grid and repetition artifacts |
| Symplectic integration, Verlet / leapfrog | Live orbital dynamics | Long-session numerical stability unlike Euler |

## 8. Data Sources

- **Stars:** Gaia catalog (DR3), including proper motions and positions; streamed through a spatial index rather than held in memory. Linear proper-motion extrapolation valid for ±1000 years from reference epoch 2016.0.
- **Planetary positions:** VSOP87 default (3000 BCE – 3000 CE); optional DE440-derived simplified elements for outer planets (Jupiter–Neptune) in high-precision mode (10000 BCE – 10000 CE).
- **Earth geodesy:** WGS84 for ECEF, real latitude/longitude for facility placement, and real terrain data for regional and aerial scales.
- **Cosmic web / supercluster:** no literal consumer-scale renderable catalog; use seeded, cosmologically plausible procedural structure and explicitly flag it as procedural rather than real data.

## 9. Environmental, Atmospheric, and Transition Visuals

### 9.1 Physical depth cues

Rayleigh and Mie scattering, the classic haze and blue-shift depth cue, requires an actual gas medium. It is physically valid only inside Earth's atmosphere, at waypoints 7–10 and during descent into 7. All other scale regimes are vacuum and require a physically valid substitute.

| Scale regime | Physical basis for depth cueing | Not this |
|---|---|---|
| Interplanetary (6) | Faint, real zodiacal dust glow | Atmospheric haze; vacuum |
| Interstellar (5–4) | Interstellar dust extinction and reddening, color excess `E(B-V)` and extinction `Aᵥ` | Fog; actual dust column density is the effect |
| Local Group / intergalactic, local (4–3) | Dust extinction/reddening plus peculiar-velocity Doppler tinting | Cosmological redshift; peculiar velocity dominates at supercluster range |
| Cosmic web, cosmological (2–1) | Cosmological redshift after Hubble flow dominates, plus raymarched seeded volumetric density fields | Literal scattering |
| Atmosphere (7–10) | Real Rayleigh blue sky/horizon haze plus Mie haze/pollution scattering | This is the regime where classic atmospheric perspective is physically correct |

Andromeda is blueshifted despite being outside the Milky Way, illustrating why cosmological redshift is not a monotonic local intergalactic depth cue.

### 9.2 Dynamic range and exposure

The Sun and background starlight differ by approximately 10 orders of magnitude in brightness. Surface daylight and deep-space starlight span roughly 30 stops. No fixed exposure can represent both correctly.

Requirements:

- Use physically driven auto-exposure keyed to the dominant light source in view: Sun, planet albedo, or ambient starlight when no brighter source dominates.
- Apply filmic tone mapping, ACES or comparable, rather than clipping.
- Model gradual dark adaptation: stars should fade in as sky luminance falls and they cross the display's effective scotopic threshold, mirroring civil, nautical, and astronomical twilight rather than popping at a fixed altitude.

### 9.3 Waypoint-to-waypoint experience

- **10 → 9, Interior → Exterior:** Artificial interior lighting to exterior ambient/daylight; primarily an exposure-adaptation cut rather than a depth-cue change.
- **9 → 8, Exterior → Aerial:** Real aerial-photography haze; contrast and saturation diminish with distance and altitude through Rayleigh/Mie scattering.
- **8 → 7, Aerial → Full Earth:** Continue the same atmospheric scattering model; sky deepens from blue to indigo to black, Earth's limb gains a thin blue glow, and curvature becomes visible near the Ká¡¡rmá¡¡n-line range. Use either 100 km, the FAI convention, or approximately 80 km, a physically motivated reanalysis threshold, without representing either as uncontested.
- **7 → 6, Earth → Solar System:** Earth recedes to a point of light; the Sun's apparent disk shrinks by inverse-square illumination behavior, planets brighten gradually as points, and faint zodiacal glow appears along the ecliptic.
- **6 → 5, Solar System → Solar Neighborhood:** The Sun becomes one point among neighboring stars. This is the exposure system's hardest transition because the hero light source changes from Sun-as-disk to Sun-as-point.
- **5 → 4, Neighborhood → Milky Way:** Individual stars resolve as components of spiral structure; dust-lane extinction reddens and dims light near the galactic plane, contributing to visible obscuration of the galactic core.
- **4 → 3, Milky Way → Local Group:** The Milky Way becomes one galaxy among Local Group members; depth comes from dust extinction/reddening and peculiar-velocity Doppler tinting, not cosmological redshift.
- **3 → 2, Local Group → Supercluster:** The Local Group becomes a point or blob among many clusters; depth cues remain dust extinction and peculiar-velocity effects.
- **2 → 1, Supercluster → Cosmic Web:** Hubble flow becomes dominant enough for cosmological redshift to serve as a monotonic depth cue. More distant filaments tint progressively redder, combined with raymarched volumetric density fields for dust-node glow and filament luminosity falloff.

### 9.4 Zodiacal light model

Version 0.4 uses an analytic zodiacal-light model in the solar-system frame, with a future replacement path to a map-based HEALPix asset without changing render interfaces. Treat this layer as reflected/scattered sunlight from interplanetary dust, not atmospheric haze.

- Base V-band surface brightness: μ₀ = 23.0 mag/arcsec^2^ at the ecliptic poles.
- Evaluate per pixel from camera ray direction transformed into ecliptic coordinates.
- Modulate intensity by ecliptic latitude and solar elongation using a forward-scattering phase function; clamp all terms to measured-plausible ranges and expose calibration parameters for photographic comparison.
- Convert magnitude surface brightness to linear radiance before exposure and filmic tone mapping.

Use the following v0.4 approximation, expressed in magnitude offset form:

\[
\mu(\beta, \theta) = \mu_0 - A_\beta \cdot \exp\left(-\frac{|\beta|}{\beta_0}\right) - A_\theta \cdot \frac{1}{1 + k \cdot (1 - \cos\theta)}
\]

where:
- β = ecliptic latitude.
- θ = solar elongation (angle from Sun to pixel direction).
- Aβ, β₀, Aθ, and k are calibrated art-and-science parameters.

Initialize with:
- Aβ = 1.5 mag, β₀ = 20^∘^ (latitude falloff).
- Aθ = 0.5 mag, k = 0.7 (forward-scattering strength).

Validate against reference zodiacal-light observations (e.g., COBE/DIRBE, HST background models) before release.

## 10. Product Decisions and Remaining Open Items

### Resolved for v0.4

#### HUD and navigation overlay

Ship v0.4 with the full minimal overlay HUD:

- **Active-frame indicator:** display active frame name, local unit system, primary reference body where applicable, and camera/ship distance or altitude expressed in that frame.
- **Time state:** display real-time or current time-compression ratio and its state-machine mode.
- **SOI handoff indication:** when the ship enters a soft-handoff blend band, present a subtle visual indicator whose strength follows blend weight. Show "Approaching / Entering [body] SOI" once blend weight exceeds 0.2; prevent noisy repeated messages through hysteresis.
- **Select-to-focus target:** display a reticle or directional marker, resolved target name/identifier, distance, and estimated arrival time throughout an automated fly-to.
- The release UI may use a fast overlay implementation. It must be decoupled from simulation state and must not change physics, timing, or frame-selection behavior.

#### Persistence

Ship v0.4 with binary autosave. Procedural regions remain unsaved because they are regenerated from deterministic seeds.

Persist:

- **Ship state:** active-frame ID, 6D position/velocity state vector in that frame, orientation quaternion, mass, fuel, and any active control state required for deterministic resume.
- **Navigation state:** select-to-focus target identifier or canonical coordinates, active transition plan if any, and time-compression mode/multiplier.
- **Metadata:** format version, master seed, real-world save timestamp, simulation timestamp, session playtime, data-catalog version identifiers, and integrity checksum.

Autosave triggers: confirmed active-frame transitions, completed SOI handoffs, beginning and completion of an automated fly-to, explicit application suspend/quit, and a periodic interval configurable by the user. Write atomically using a temporary file plus rename; retain at least two rotating recovery snapshots.

#### Audio

Audio is explicitly deferred to v0.5. Version 0.4 ships silent except for platform-required accessibility/system feedback, if any. The renderer and simulation must not depend on an audio clock.

#### Scale hierarchy

Waypoint 6 ("Solar System") raised to 10^14^ m to fully contain the heliopause (nose, flanks, and tail), based on Voyager 1/2 crossings at ~119–121 AU.

#### Galactic dynamics

Explicit Miyamoto-Nagai disk + NFW halo potential formulas provided for Milky Way kinematics, with typical scale parameters (a = 6.5 kpc, b = 0.26 kpc, r_s = 20 kpc).

#### Ephemeris policy

VSOP87 default for all planets (3000 BCE – 3000 CE); optional DE440-derived simplified elements for outer planets (Jupiter–Neptune) in high-precision mode (10000 BCE – 10000 CE).

#### Gaia proper-motion validity

Linear proper-motion extrapolation valid for ±1000 years from Gaia DR3 reference epoch 2016.0; beyond this, display a warning and optionally switch to procedural jitter.

### Still open

The following items remain open and must not be silently dropped:

- **Visual/rendering style:** Physically based versus stylized, including the acceptable degree of perceptual exaggeration for readability.
- **Facility anchor:** Exact Earth latitude, longitude, ellipsoidal height, datum, terrain source, and any fictionalization/security constraints.
- **Player craft definition:** Mass range, thrust model, propellant model, maximum acceleration, navigation assists, collision policy, and landing/atmospheric-flight scope.
- **Terrain data policy:** Dataset, licensing, offline cache strategy, terrain vertical datum, imagery source, and mesh/texture LOD budget.
- **Network/content policy:** Offline-first packaged data versus optional network streaming, cache size, and behavior when real-data tiles are unavailable.
- **Validation plan:** Numerical test tolerances, reference screenshots, orbital regression tests, precision tests at every frame boundary, and performance targets by supported GPU tier.

---

*End of draft v0.4 — includes heliopause-containing Solar System waypoint (10^14^ m), corrected zodiacal forward-scattering model, HEALPix resolution table, Gaia proper-motion validity window, explicit galactic potential formulas, VSOP87/DE440 ephemeris policy, and Laplace vs Hill sphere clarification.*
