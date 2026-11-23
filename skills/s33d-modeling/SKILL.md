---
name: s33d-modeling
description: Create, revise, visually verify and save S33D geometry from dimensions, natural language or reference photos through the running S33D MCP connection. Use for photo reconstruction, CAD modeling and continued edits to an existing model.
---

# S33D modeling

Guide version: 2026-11-17.2. The same files are embedded in the shipped MCP workflow tool; personal skills and a development checkout are not runtime requirements.

## Start and route

Discover the connected S33D tools, retrieve s33d_workflow, call s33d_describe for the actual method catalog, then use document.summary via s33d_execute. A host exposing only the older three tools can retrieve the same guide with s33d_execute(method="workflow.get", params={"mode":"auto"}). Legacy empty s33d_state still returns full geometry; use it only when that detail is needed. A disconnected desktop must be opened/connected using Settings > Agent Connect. Do not hardcode ports, install paths or API methods. Keep user drafts intact; busy means finish/cancel the desktop tool before proceeding.

Read [modeling-strategy.md](references/modeling-strategy.md) for the common evidence, construction and acceptance gates. Maintain its compact execution state from actual observations, including the largest deviation, decision and next recheck. The guide's version and source hashes identify the instructions; they do not prove successful execution.

- Text/dimensions: read [modeling-patterns.md](references/modeling-patterns.md).
- Photo reconstruction: you interpret the photos visible in your chat, then call S33D to model and obtain comparison images. Importing photos into S33D is optional; do not require asset IDs or a second upload. Read [photo-reconstruction.md](references/photo-reconstruction.md), then [visual-validation.md](references/visual-validation.md).
- Continued edit: inspect current stable IDs, reuse existing dimensions/photos/accepted decisions, and read only relevant references. Do not restart the model.
- Equation curves/surfaces or offset jobs: read [interface-contracts.md](references/interface-contracts.md) and the live catalog. Without filesystem skills, retrieve this reference with s33d_workflow(mode="geometry").

Use the authorization already given. Ask only about missing facts that materially affect correctness or a newly out-of-scope action. Do not demand a second approval for a fully specified request. Approximate appearance modeling can proceed at a disclosed relative scale; engineering dimensions and tolerances need actual evidence.

## Shared execution contract

All lengths are mm and angles are degrees. Establish object axes/datum and retain a compact dimension/evidence ledger. Body IDs are stable; topology indices and sketch regions must be queried after geometry changes. World Front is +Z; object semantic front is task specific.

Choose idempotency_key before every write. After an ambiguous timeout, retry exactly the same key, method and parameters on the same listener within ten minutes of completion. Queued expiry/cancel never executes; in_progress cannot be interrupted. After restart/expiry inspect current objects before repeating work. Do not treat chat summaries or checkpoints as isolated rollback transactions.

Use optional _expected:{document_id,revision} in command params to reject stale writes atomically on the desktop. document.summary provides these tokens; opening/replacing a document changes its identity. Undo/Redo advances revision rather than restoring an old token. View changes have a separate view_revision.

Execute one meaningful operation at a time and verify document.summary for IDs, names, bounds, feature kinds and visibility. Use body.get for needed parameters/topology, not repeated full-document B-Rep. Keep Boolean operands until verified. Undo only the operation performed by this task, immediately and after checking no user work intervened; never undo a guessed number of steps.

For appearance tasks, compare actual native s33d_render images as described in visual-validation.md. Numeric checks and visual checks establish different facts. Major shape/structure mismatches block detailing; each image must lead to retaining, adjusting, reverting, changing construction or a concrete blocker. Color can be set with body.material.set using explicit IDs and RGB in 0..1. Preserve unrelated objects and settings.

## Deliver

Fit the full result, recheck the original target independently of the construction checklist, and save/export to the destination already authorized. If no save destination exists, ask once. Report appearance, geometry, dimensions/function, editability and save/reopen separately, including scale, actual geometry type and unverified requirements. Core shape failures remain unfinished. A plausible picture is not proof of a closed printable solid. Leave the editable model open; do not change external Agent configuration during ordinary modeling.
