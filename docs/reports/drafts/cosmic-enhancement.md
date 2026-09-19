# **Cosmic Web — Unified Technical & Architectural Specification (Merged Report)**

## **1. Purpose & Scope**
This document describes the **complete architecture** of the cosmic web system:  
generation → classification → descriptor assembly → enrichment → rendering → post‑processing → gameplay integration → future roadmap.

It merges all prior reports into one consistent, non‑redundant specification.

---

# **2. Core Strengths of the Current System**

### **2.1 Physically Motivated Generation**
The pipeline is scientifically grounded:

- **Gaussian initial field** (Irwin‑Hall shaping, no transcendentals)  
- **Zel’dovich displacement** on a periodic 128³ lattice  
- **T‑web tidal tensor classification** (Forero‑Romero method)  
- **Press–Schechter mass function** for halo masses  

This is the same conceptual pipeline used in cosmological simulations, simplified for real‑time use.

### **2.2 Deterministic Replay**
Every stage is deterministic per `(seed, UNIVERSE_VERSION, params)`:

- Irwin‑Hall RNG  
- Fixed‑sweep Jacobi eigen solver  
- NGP mass deposit  
- No sin/cos or convergence branches  

This guarantees **bit‑identical replay across platforms**, critical for multiplayer, save integrity, and debugging.

### **2.3 Clean Separation of Simulation vs. Visual Enrichment**
The `WebDescriptor` contains only gameplay‑relevant data:

- Nodes  
- Filament links  
- Glow points  

All visual enrichment (braids, grain, impostors) is **render‑only**, never used for selection or physics.

### **2.4 Mature GPU Rendering Architecture**
- Additive pipelines on a deep indigo clear  
- Hubble redshift tinting in vertex shader  
- HDR bloom chain with strict “write‑once, read‑many” rule  
- No depth write → natural additive accumulation at nodes  

This produces a luminous, cinematic cosmic web with strong depth cues.

---

# **3. Generation Pipeline (Canonical Specification)**

### **Stage A — Gaussian Initial Field**
- 128³ lattice, 4 Mpc/cell  
- Irwin‑Hall shaping → N(0,1)  
- Dyadic smoothing approximating ΛCDM spectrum  
- Output: potential field Ψ

### **Stage B — Zel’dovich Displacement**
\[
x = q - D_+ \nabla \Psi,\quad D_+ = 3.0
\]
- Central differences for gradient  
- NGP deposit into Eulerian density  
- No smoothing → preserves high dynamic range

### **Stage C — T‑web Classification**
- Tidal tensor from second derivatives  
- Fixed 8‑sweep Jacobi solver  
- Classification by eigenvalue count:  
  - 3 = NODE  
  - 2 = FILAMENT  
  - 1 = SHEET  
  - 0 = VOID  
- Deep voids skip eigensolver  
- Peak extraction → ~6000 nodes  
- Press–Schechter mass assignment

### **Stage D — Descriptor Assembly**
- Nodes: 5e12–3e14 Msun  
- Filament links: ~20,000  
- Glow points: up to 150,000  
- Home node near center  
- Void fraction: 60–90%

---

# **4. CPU Enrichment Layer**

### **4.1 Braided Filament Strands**
- 1–3 strands per link  
- Shared trunk wander  
- Per‑strand twist  
- Sinusoidal taper  
- Indigo → cyan‑violet color ramp  
- Alpha 0.18–0.58

### **4.2 Grain Cloud**
- Up to 800,000 points  
- Irwin‑Hall jitter  
- Lavender‑white sprites  
- Alpha 0.08

### **4.3 Node Impostors**
- Hot core (emissive, bloom target)  
- Soft halo (world‑scaled, pale cyan)

### **4.4 Glow Points**
- Lavender haze along filaments

---

# **5. GPU Rendering & Post‑Processing**

### **5.1 Pipelines**
- **Glow**: additive point sprites  
- **Webline**: additive line list  
- **Map**: alpha‑blended player marker  

### **5.2 Redshift Tinting**
\[
z = \min(redshift \cdot \max(clip.w, 0), 0.5)
\]

### **5.3 HDR Bloom Chain**
- Bright extract  
- Two‑scale separable Gaussian blur  
- ACES tonemap  
- Strict write‑once rule (Intel UHD 620 workaround)

---

# **6. Known Gaps & Architectural Risks**

### **6.1 Sheets Are Classified but Never Rendered**
SHEET cells are computed but unused visually.  
This is the **largest missing structural component**.

### **6.2 Threshold Sensitivity**
`web_threshold = 0.06` is resolution‑dependent.  
Changing lattice resolution will alter topology.

### **6.3 Linking Graph Regularity**
Capping links at 8 and 60 Mpc produces a uniform mesh.  
Real cosmic webs have dominant long filaments.

### **6.4 Grain/Glow Overdraw**
~1M additive sprites → fill‑rate pressure on integrated GPUs.

### **6.5 Zel’dovich Shell‑Crossing**
At D+ = 3.0, multi‑streaming blurs dense nodes.

### **6.6 No LOD System**
All detail is rendered at all distances.

---

# **7. Enhancement Roadmap (Merged & Prioritized)**

## **Phase 1 — Visual & Rendering Upgrades (Short‑Term)**

### **1. Replace LineList with Screen‑Space Ribbons**
Use quad strips with Frenet frames for filaments:

- Variable width  
- Anti‑aliased  
- Density‑based brightness profile  

### **2. GPU Compute Shader Grain Generation**
Move grain generation off CPU:

- Lower memory  
- Higher particle counts  
- Dynamic density modulation

### **3. Multi‑Scale Bloom Pyramid**
Add 4‑level downsample/upsample chain:

- Larger halos for massive nodes  
- Optional anamorphic streaks

### **4. Sheet Rendering Pass**
Visualize SHEET cells:

- Sparse translucent patches  
- Triangulated surfaces  
- Faint additive glow

---

## **Phase 2 — Simulation & Structural Upgrades (Mid‑Term)**

### **1. 2LPT Displacement**
Add second‑order term:

- Sharper filaments  
- More realistic tidal curvature  
- Tighter cluster cores

### **2. Adaptive Nested Grids**
Local 64³ or 128³ sub‑grid near player:

- High‑res detail when close  
- No global memory cost

### **3. Filament Skeleton Simplification**
LOD skeleton for far views:

- Merge collinear links  
- Drop low‑density stubs

### **4. Dominant Long Filament Pass**
Relax 60 Mpc cap for massive node pairs.

---

## **Phase 3 — Volumetrics, Scientific Layers & Interactivity (Long‑Term)**

### **1. Volumetric Raymarching**
Use 128³ density field as 3D texture:

- WHIM gas fog  
- Soft volumetric filaments  
- Stencil‑masked raymarch

### **2. Full LOD System**
Three tiers:

- Far: skeleton + impostors  
- Mid: braids + grain  
- Near: galaxy billboards + high‑res sub‑grid

### **3. Galaxy Billboards**
Replace grain with galaxy sprites at <5 Mpc.

### **4. Scientific Data Overlay**
Load SDSS / 2MRS / Illustris subsets:

- Real galaxies  
- Redshift‑space distortions  
- Toggleable comparison mode

### **5. Void Surface Extraction**
Marching cubes iso‑surfaces:

- 10% density boundary  
- Fresnel edge glow  
- Explicit foam‑like structure

### **6. Temporal Evolution**
Precompute snapshots at multiple D+ values:

- Time scrubbing  
- Structure formation animation

### **7. Dynamic Interaction**
- Player mass perturbation of displacement field  
- Gravitational lensing shader

---

# **8. Immediate Action Items (Unified Priority List)**

1. **Filament ribbons (quad strips)**  
2. **Compute‑shader grain generation**  
3. **Sheet rendering pass**  
4. **LOD for grain/glow**  
5. **Dominant long‑filament linking**  
6. **Volumetric raymarching prototype**  
7. **Supercluster clustering + labels**  
8. **Guided camera tours**  
9. **Cross‑platform determinism hash tests**

---

# **9. Final Summary (TL;DR)**

The cosmic web system is already a **physically grounded, deterministic, cinematic large‑scale universe renderer**.  
The next evolution focuses on:

- **Thickened filament geometry**  
- **Volumetric gas rendering**  
- **Sheet visualization**  
- **LOD for multi‑scale exploration**  
- **Scientific overlays**  
- **Temporal evolution**  
- **Dynamic interaction**  

This merged report is now your **single authoritative specification** for the cosmic web architecture and its future roadmap.
.
