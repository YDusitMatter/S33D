# S33D Modeling Patterns

Use the common gates and execution state in [modeling-strategy.md](modeling-strategy.md). Reuse the user's supplied brief and authorization; ask only for missing material facts. Always prefer the live `s33d_describe` catalog over examples here.

## Dimension ledger

Maintain a small internal ledger before construction:

- requested unit and millimeter conversion factor;
- overall X/Y/Z envelope;
- datum/origin and primary orientation;
- profile dimensions, thicknesses, radii, diameters, angles, and offsets;
- counts and spacing for repeated features;
- separate-part versus joined-body intent;
- assumptions within the authorized scope, with evidence/confidence and unresolved material unknowns.

Compare final world-space bounds and feature state against this ledger. Do not confuse radius with diameter or total count with the number of added copies.

## Construction choices

### Direct primitives

Use direct primitives when a body is fully described by primitive parameters. Create at a simple datum, verify its bounds, then transform it into place. For a frustum, confirm which end receives the top radius and which direction is height.

### Planar profiles

Use a sketch and extrude for plates, brackets, slots, polygonal parts, and planar hole layouts. Put outer boundaries and inner loops on the same intended plane when the API can represent them. After sketch creation, inspect the reported closed regions rather than guessing a region index. Confirm whether the selected region includes or excludes each inner loop before extrusion.

### Revolved profiles

Use a half-section and revolve for shafts, bushings, knobs, bowls, and other axisymmetric bodies. Keep the profile on one side of the axis unless a crossing is intentional and supported. Verify the axis point, axis direction, angle, and resulting diameter.

### Compound and repeated geometry

Decompose compound shapes into the fewest stable bodies that preserve design intent. Position bodies using transforms and apply union, subtract, or intersect only after operand bounds overlap as expected. For repeated features, calculate centers from a declared datum; do not accumulate offsets from the previous copy because rounding errors compound.

## Reconstructing from images

First identify whether the reference is orthographic, perspective, sectioned, or an exploded view. Separate observations from assumptions:

- silhouette and visible edge relationships are observations;
- absolute scale, hidden depth, wall thickness, rear features, and exact radii are usually assumptions;
- perspective lengths are not measurements unless a scale or calibrated view is supplied.

Reuse any known dimension that spans a clearly identifiable feature. Scale other visible dimensions only as provisional ratios and disclose their uncertainty. Multiple orthographic images may resolve depth and hidden geometry; conflicting views require user direction before construction.

For an authorized approximate visual likeness without engineering dimensions, disclose a provisional 100 mm longest dimension and continue. Prioritize silhouette, structure and identity impact; a small distinctive opening may precede a large low-impact detail. Follow photo-reconstruction.md. Do not claim manufacturing accuracy from a single image.

## Verification and rollback

After each create, transform, extrude, revolve, or boolean operation, compare reported bounds with the expected numeric envelope. Also verify that source bodies have the intended visibility and that stable IDs used by later commands still exist.

If verification fails, reconcile current state first. Diagnose parameter, shared constraint, representation, camera, reference conflict or API state. Revert only this task's failed operation after confirming no user work intervened; verify the earlier state. Choose a correction at the causal level, record the gate decision and recheck affected constraints/views. Repeated changes without improvement require another hypothesis or construction. Report a specific unsupported capability or material ambiguity when it blocks progress; do not substitute unrelated visible geometry.
