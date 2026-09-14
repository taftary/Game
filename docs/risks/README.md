# Risks and open questions

1. Seamless descent on low-end phones is the top risk — mitigation: aggressive LOD, fades, small planet radius, tier gating.
2. Bit-stable procedural noise across ARM/x86 — mitigation: quantized outputs, hash tests per platform.
3. Custom engine cost (UI, tooling, asset cooker) — mitigation: tiny v1 scopes, `tools` grows with need, cut volumetric/clouds/multiplayer early.
4. Touch UX for colony management — mitigation: tap-order model, large targets, early playtest on phones.
5. Vulkan on older Android / Apple (MoltenVK) devices — mitigation: explicit device allowlist, tier gating, graceful "unsupported" screen; GLES fallback only by ADR, never silent.
6. Save migration churn — mitigation: version stamp from day one, migrator in `tools`.
7. VR scope creep — mitigation: VR is design-constraint only until M8 audit passes.
