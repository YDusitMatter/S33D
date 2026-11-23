# S33D Alpha Preview 0.1.0-alpha.1

Candidate refresh: 2026-11-23. The preview name and version are unchanged.

## 2026-11-23 normal Windows Alpha Preview channel

- Before copying an update, Setup checks write access to the selected installation's application and Agent executables. Busy or non-writable executables stop Setup before application files are replaced; Setup does not force-close user programs.
- The normal per-user Windows channel uses a fixed application identity and installation directory so later packages can update the same installation. Installation does not launch S33D or register an Agent automatically.
- Includes the November 22 modeling/menu/export updates and synchronized manuals. Windows native dependency diagnostics are built with neutral source paths, including UTF-16 assertion messages.
- This candidate is unsigned. Local installation and upgrade checks do not replace clean-machine, physical display/GPU or real visual Agent host acceptance.

## 2026-11-22 selected-surface menu and export defaults

- A selected bounded planar face or closed sketch region offers Sketch on this surface, Extrude (E), Pocket (P), Revolve and Deselect surface (Esc) in its context menu. Supported cylindrical walls retain the Generate thread route.
- Model export suggests the current project name and puts STEP (.step) first; .stp and the other supported model and drawing formats remain available. Agent body.export continues to use its path extension or explicit format parameter.
- English/Chinese Help and Agent instructions match these desktop changes. This update adds no MCP methods.

## 2026-11-20 modeling and export update

- STEP export can repair supported folded, overlapping seams left by older mesh Boolean operations. Repair remains bounded by welding tolerance and must pass topology, bounds, volume, boundary and OCCT solid validation. The desktop and `body.export` share this path; invalid geometry is still rejected.
- Pocket can infer an unambiguous visible target from a selected closed profile, its supported associations or feature history, and geometric checks. Ambiguous targets still require explicit selection. P opens Pocket for a selected closed profile; Q remains available.
- Leaving Sketch with Esc preserves completed entities; an active drawing chain is ended first. Draw 3D can retain the current primitive after Apply when leaving with Esc. Completed results remain available to Undo/Redo.
- Sketch properties support a locked vertex. The default is the first placed point for supported lines, arcs and rectangles, and the center for polygons. Parameter edits and rotation preserve that point; moving the sketch moves its lock with it. Lock choices persist in projects, with defaults for older files.
- English/Chinese Help and Agent instructions describe these workflows. Pocket target inference and vertex-lock controls are desktop behavior; this update does not add new MCP methods.

## 2026-11-19 desktop and Help update

- The top-right workspace switch shows or hides the grid and RGB axes together. View uses the same setting, and Re-enable all hidden tips restores both guides.
- File > Open Recent retains the ten most recently used project paths across restarts. Opening a recent project keeps the usual unsaved-changes confirmation and background loading.
- Closing S33D ends its connected MCP bridges while preserving confirmation for unsaved projects. After reopening S33D, restart the Agent if it does not reconnect automatically.
- English/Chinese Help and the main manual use Settings > Agent Connect, list all six MCP tools, and describe the workflow, compact document summary and native image checks. The bridge, configuration templates and guides are supplied together; installation does not register an Agent automatically.

## Updated behavior

- The installer, application executable/window, Start menu, desktop shortcut and uninstall entry use the same transparent S33D artwork. The iron-walnut revision approved on 2026-11-12 uses silver S3 pipes and leaves, copper 3D text, a fuller cracked iron shell and roots joined to the silver 3 through standard pipe fittings. The source kit includes the approved master, ten-size ICO assets, provenance/hashes and a reproducible conversion script.
- A borderless startup image appears in the center of the primary display for at least two seconds after its first complete frame, remaining visible while a startup file takes longer to load.
- Project loading, mesh import and supported modeling jobs report background stages or file-read progress; failed or cancelled results are not committed.
- Thread generator parameters use two columns and expand into the available space, with Warning, Correct and Generate below the parameter scroll area.
- Correct according to nominal diameter changes the actual bore or blank while retaining the requested nominal diameter and all thread/printing parameters. Bore diameter is `diameter - 2*depth`; external blank diameter is `diameter + length*taper`.
- Certified complete Sketch Extrude/Pocket cylindrical walls support correction and direct diameter editing, including inclined, symmetric and reversed extrusion and blind/through pockets. The result is an exact direct-edit solid; shared source sketches remain unchanged. Faceted conversion is limited to 100,000 input triangles; raw imported mesh walls remain read-only.
- Complete uncut cylindrical bands remain independently selectable after partial threading. Start offset is measured from the selected band's origin. Requery Agent targets after geometry changes; `thread.apply` does not resize a mismatched bore automatically.
- English/Chinese Help, Agent instructions and the printing/correction guide describe the updated workflow. Agent templates and the modeling skill are supplied as documentation; installation does not register or configure an Agent automatically.

## Preview limits

This refresh does not add a manufacturing tolerance class, thread gauge certification or physical printing acceptance. Threads remain sampled geometry. Complex operations can fail validation, and indivisible native calculations may continue after cancellation while their results are discarded. Startup speed has no controlled before/after benchmark. Final installer and clean-machine acceptance must be recorded for the actual candidate artifacts before distribution.

A package built without a signing certificate is unsigned; do not disable operating-system security checks. See the included user manuals, `Docs/ThreadPrinting.md` and `Docs/AgentControl.md` for usage and limits.


## 2026-11-17 Agent photo handoff clarification (Windows local test)

- The visual Agent interprets photos attached in its chat, uses S33D to model, and compares native model images for corrections. Importing photos into S33D is optional project storage or alternate delivery; no imported asset or second upload is required for chat images the Agent already sees.
- Guide 2026-11-17.2, MCP connection instructions, English/Chinese manuals and Agent Connect share this flow. Copy modeling prompt now works without imported photos and includes project reference IDs only when present.
- The Windows local test package is rebuilt from these sources. Previous Windows/Linux artifacts remain preserved; Linux is not rebuilt.

## 2026-11-17 shared modeling strategy (Windows local test)

- The shipped skill and MCP guide share evidence, construction, stage-gate and failure-diagnosis rules. Adjacent regions use common constraints; core shape failures block detailing.
- Guide 2026-11-17.1 adds per-source hashes and a compact Agent-maintained execution-state template. `workflow.get` supports older three-tool hosts with the updated desktop. This is guidance and provenance, not an automatic visual-quality verdict or a persisted workflow engine.
- Windows packaging includes the common strategy reference. Linux packaging and its last tested artifacts are unchanged in this task.

## 2026-11-16 photo/model integration

- Settings > Agent Connect brings existing Agent registration, reference-photo import and an associated start prompt into one entry.
- A unified modeling skill and the embedded MCP workflow cover dimensioned modeling, photo reconstruction and continued edits. Ordinary use requires no separate personal skill installation.
- Native MCP reference/model images, camera read/set, compact document summaries, atomic RGB material updates, portable project images, and optional document-version guards are available. Existing three tools remain compatible.
- Native images use the viewport render path with bounded output sizes, preserve desktop view/selection/model/history/save state, and include snapshot metadata. PNG/JPEG references apply EXIF orientation and are embedded on project save.
- Host support still requires a visual Agent with MCP image display. The attachment fallback is one photo import in S33D followed by the associated modeling prompt. Advanced cross-restart workflow recovery, photogrammetry and arbitrary surface solidification are deferred.
