# S33D User Guide

**Exit confirmation:** before a workspace double/triple-click exits a tool, a dialog names the tool and explains that the current state will be kept. OK applies and finishes the tool, then performs the original face/object selection; Cancel or Esc stays in the tool. Check “Don't show this again” and confirm to remember that preference. Unchecked prompts remain enabled. Settings > Re-enable all hidden tips restores both this confirmation and Getting Started tips, without changing other preferences. Ordinary idle selection has no exit prompt.

**Exit a tool and select:** in the 3D workspace, double/triple-click applies and finishes the current valid state (Applied and Done), collapses tool menus, then selects the surface/whole object under the pointer. Normal Exit and tool switching also accept the latest parameters. Already applied arrays and relative transforms are not repeated. Invalid parameters or failed geometry keep the editor open with a reason; background work finishes before exit. Explicit Cancel actions still discard drafts. Consecutive points while constructing sketch geometry or placing/sizing a Draw 3D primitive retain their original behavior. The 2D drawing canvas and object-list double-click editing remain unchanged. **In Draw 3D, single-click a body to select and edit it**; Getting Started also explains this.

## Groups, center-axis alignment and fastener plug-ins

**Groups:** select at least two Objects rows (Ctrl-click, Shift-click or Ctrl+A), then use Group on the left toolbar, Edit > Group, or right-click the selected rows > Group. A collapsible group header appears in Objects. Click it to select all members, including hidden members; Ctrl/Shift-click toggles the group. Individual member rows remain selectable. Shift ranges follow the displayed group order. Move/Rotate/Scale and other compatible commands operate on the selected members. Grouping does not fuse geometry or change object IDs. Ungroup on the left toolbar, Edit > Ungroup, or the group header's Ungroup action removes the grouping and retains all objects. Groups are saved in the project and participate in Undo/Redo. A group header also offers bulk Show, Hide, Color and Delete group and objects; the last action deletes its members. Single-object tools reject conflicting group selections and explain why.

**Center-axis alignment:** open Align, add the objects, then check Center-axis alignment. Pick a cylinder wall, straight model edge or straight sketch line as the target axis, or enter the line equation `P(t) = (X,Y,Z) + t(a,b,c)`. X/Y/Z locate a point in the current display unit; a/b/c are a nonzero direction vector, not angles. A picked cylinder stays fixed. Alternatively lock a reference in the selection list and click Use locked object's axis. Automatic cylinder axes uses the longest supported outer cylindrical wall, falling back to an inner wall; ambiguous multi-axis parts should use explicit picking. For other objects choose a world X/Y/Z direction through their bounding-box center. Apply rotates each source axis onto the target line and removes its perpendicular offset, preserving its position along the line and its size. Axis signs are treated as unoriented, avoiding unnecessary end-for-end flips. The entire operation is one Undo step. Uncheck Center-axis alignment to return to Min/Center/Max alignment.

**Appearance and toolbars:** the original text-only layout is now named Simple. Choose Simple, Precision Classic, Graphite Studio or Porcelain Workshop in Getting Started or Settings > Appearance; the choice becomes the saved default. Getting Started follows the current appearance. Opening the Sketch category closes Draw 3D, and vice versa; collapsing a category alone retains its draft. Each enabled plug-in has a compact toolbar below the menu bar. Click its triangle/name to expand or collapse the full tool; disabling the plug-in also closes its window. Collapsing retains its parameters.

**Screw Generator / Nut Generator:** enable either under Plug-ins, then expand its toolbar. Screw styles: hex, socket cap, countersunk, thumb screw and stud. Nut styles: hex, square, round and flange. Select M3/M4/M5/M6/M8/M10/M12 nominal metric presets or Custom, then edit diameter, pitch, length, head/body dimensions, socket, thread length, hand, radial clearance and entry chamfer. The standard head/nut dimensions are references; countersunk, thumb, square, round and flange styles use custom starting shapes. These are idealized geometry recipes, without thread tolerance classes or strength certification. Countersunk length includes its head; other screw lengths are measured under the head; nut length is its height. Coordinates locate the base, with the initial axis along +Y. Displayed dimensions follow Settings units; internal storage remains mm.

Generate computes a real helical mesh solid in the background and commits one Undo step. Finish the current interactive tool first. A failed calculation or a document change before completion leaves the document unchanged. Keep S33D open until generation finishes; saving waits for completion. Select a generated fastener, reopen its plug-in, adjust parameters and click Update selected to keep its ID/name/color, or Create copy for a separate object. Edits, transforms, parameters and groups survive project save/reopen. Uncheck Modeled thread to create a blank without the helical cut; a nut blank retains the minor-diameter bore.

**Printing:** Print coarse 12 x 2 provides a larger coarse-pitch starting pair. Print fit adds radial clearance per part (the screw shrinks and the nut bore grows); clearance is independent of pitch and flank angle. Matching pitch, nominal diameter and hand are essential. Both nut ends have entry chamfers and the thread cut reaches the physical ends. Print a short fit sample before a full part; material, layer height, orientation and printer calibration determine the clearance needed. These sampled mesh solids can be exported to STL; they are not analytic manufacturing threads. Detailed preset sources: [Fastener generator notes](FastenerGenerators.md).

## Thread generator

Double-click a cylinder side or bore wall to select a recognized complete cylindrical surface. Only then does **Properties > Thread generator** appear; click to expand. Right-click **Generate thread** also opens it. Object-only selections, planar caps and unsupported surfaces hide the entry. End caps, neighboring holes and opposite walls remain separate.

A visible on-body sketch matching a bore's circular opening does not block hover or double-click selection of the inner wall. To edit that sketch, select it in Objects and choose Edit Sketch. Other sketch regions keep their existing selection priority.

The panel first shows the object's name, bore/cylinder role, diameter, axial length, axis and origin, followed by Preset. The menu shows only metric, UNC/UNF, BSPP, BSPT and personal presets compatible with the current blank/bore and axial range, sorted by geometric match, then Custom. The selected preset name remains visible; changing any thread parameter switches to Custom.

Customize nominal diameter, pitch, length, offset, depth, angle, root flat, hand, groove clearance, radial mating allowance per part and taper. Model lengths follow Settings mm/inch; nozzle and layer controls always use mm. Rounded Whitworth fixes the angle to 55 degrees and depth to 0.640327P. The required blank/bore is shown: external cylinders match the major diameter; holes match the minor bore, rather than the nominal diameter. **Generate thread** cuts only the selected wall in one Undo step. Remaining supported walls can still be machined after saving and reopening.

The former 3D-print preset is removed. Click **suggest for 3D printing** below Nominal diameter to update print-specific pitch, tooth shape and mating allowance. The adjacent **...** opens a separate settings window. Defaults are **FDM, 0.4 mm nozzle, 0.2 mm layers**; edit nozzle/layers or choose **Resin** (0.05 mm layers). Close the window, then click suggest to refresh the thread; settings alone leave thread parameters unchanged. FDM uses broad crest/root flats, a 90-degree included angle and 45-degree flanks, assuming an upright thread axis. Resin uses truncated 60-degree teeth. Nominal diameter, hand, length and offset remain unchanged. Red warnings explain mismatched bores, invalid dimensions/range, excessive layers, undersized details or exhausted tooth engagement. Invalid geometry disables generation.

Mating screws and nuts need the same nominal diameter, pitch, hand and radial allowance c per part. The complete male profile shifts inward c and the female profile outward c, giving approximately 2c combined radial allowance. Groove clearance is a separate parameter. Reference screw crest and nut root diameters are shown. Suggestions are calibration starting points: print a short mating pair for the material, orientation and machine. See [print suggestion notes](ThreadPrinting.md).

At the bottom, **save as preset** opens a separate window. Enter a unique name and click **Save** to save all thread parameters, wall role and print settings to `thread_presets.json` in the user configuration directory. Presets survive restart and appear only on compatible surfaces. Existing names are not overwritten.

Supported inputs include complete cylinder/bore walls in STEP, even when encoded as B-splines or divided into connected faces, plus recognized Draw 3D/simple cylinder features. A screw shank is independent of its head and shoulders; a tube's inner and outer walls remain separate. Imported surfaces are fitted within 0.0001 mm and accepted only when their connected boundary forms two end rings. Complete faceted STL cylinder/bore bands can also be selected after geometric checks for closed end rings and full-wall coverage. Bounded mesh fit error is at most 1% of radius and 0.02 mm; this does not recover original CAD accuracy. Incomplete arcs and interrupted walls are rejected. The result is a **sampled mesh solid** replacing the original analytic shape. Save a copy before machining if downstream work needs analytic surfaces. These are geometric presets, without manufacturing tolerance classes or gauge certification. BSPT has 1:16 diameter taper, and its diameter is defined at the start offset, not the pipe gauge plane.

Length must be greater than pitch and fit the available cylinder after the start offset; pitch is at least 0.2 mm. The requested length divided by pitch is limited to 125 turns, sampled at 96 steps per turn. The cutting tool extends one turn past each requested end, then is clipped to the interval from offset to offset + length. These two extra sampling turns do not reduce the 125-turn request limit. When this interval touches a physical end face, the grooves open through that end without leaving an uncut collar. Offset and length still allow partial threads, without cutting outside the chosen interval. A production runout or entry chamfer is not added automatically. No live thread preview is provided.

## Objects list range selection

Click an object to set the range anchor. Shift-click selects all selectable objects between that anchor and the clicked row, including both endpoints; repeated Shift-click changes the range without moving the anchor. Ctrl-click toggles individual objects; Ctrl+Shift-click adds a range. Hidden objects remain selectable in the Objects list, including as range anchors, endpoints and intermediate rows; selecting them does not show them. Use the batch Visible action to show the selected group. Sketch child rows use a separate range within the same Sketch, including Pick, Move, Rotate and Align. Object and sketch-element selections are not combined across levels. Object ranges work in ordinary selection, Align and Union; finish incompatible single-target tools first. Workspace click gestures are unchanged.

With multiple objects selected, the Objects right-click menu offers batch Group/Ungroup, Visible, Color and Delete actions, with a selected-object count. It targets the selected group even when opened over another row; Rename remains a single-object action. Mixed visibility shows an indeterminate checkbox: click to show all, then click again to hide all. The color dialog keeps the group captured when opened. Each batch creates one Undo step; deleting multiple objects requires finishing incompatible tools first.

This guide covers the main S33D workflows. Use the search box at the top of the Help menu to find a tool, action, or shortcut, then open a compact topic for its instructions.


**Numeric tool entry:** Extrude, Pocket, Revolve, Draw 3D precision, transforms, Array, Chamfer/Fillet, Offset/Shell and plane equation tools focus their first enabled numeric field once when activated. Type to replace its selected value. Dragging in the workspace releases that focus; it will not return until another tool activation. Extrude/Pocket dragging changes the draft depth and its field together, including head-on views; release fixes the value, Done commits it.

**Clipboard:** Edit > Copy / Cut / Paste and Ctrl+C / Ctrl+X / Ctrl+V operate on one selected object while no tool is active. Cut retains its geometry in the internal clipboard and deletes it in one Undo step. Text fields keep ordinary text clipboard behavior. Paste uses the existing 20 mm X offset.

**3MF geometry import:** Core mesh build items, nested components and Production cross-part references are supported, including units, instance transforms and mirrored winding. Unbuilt resources do not create extra objects. The Z-up package is rotated into S33D Y-up, then grounded/centered as other mesh imports, retaining relative positions. Printer/slicer settings, textures and color assignments are not imported. Unsupported required extensions, encrypted/ZIP64 packages, corrupt archives, invalid indices and component cycles report errors without partial objects. Limits: 256 MiB package, 64 MiB per model XML, 128 MiB expanded model XML, 2 million expanded vertices and 4 million triangles. Import is one Undo step.

**Startup and appearance:** New or missing appearance settings default to Graphite Studio; saved explicit choices remain selected. The centered splash preserves the physical 768 × 1024 reference proportion on a 3840 × 2160 work area, scales with resolution and fits the available work area without changing workspace DPI.

## 1. Interface

- The modeling toolbar is on the left and contains Sketch, Draw 3D, Extrude, Boolean, Mirror, and other commands.
- The 3D workspace is in the center. Use it to inspect geometry, select edges, surfaces, or objects, and manipulate the gizmo.
- View, Properties, Beginner Guide, and History are on the right.
- The floating Objects window manages selection, visibility, names, colors, and deletion. Drag its title bar to reposition it and scroll inside the list to reach more objects.
- The status bar shows the current hint, unit, step size, and snap state.
- The window title and Help > About S33D show the build number. The Windows installer filename includes the same number.

## 2. Selection

- Single-click an edge in the workspace.
- Double-click a surface in the workspace.
- Right-click a selected bounded planar face or closed sketch region for Sketch on this surface, Extrude (E), Pocket (P), Revolve, and Deselect surface (Esc). A supported cylindrical wall offers Generate thread.
- Triple-click an entire object in the workspace.
- Hold Ctrl or Shift to add or remove items from a multiple selection.
- Ctrl+A selects all visible solid objects. If the current tool cannot accept that selection, S33D clears the conflict and explains why.
- In the Objects window, click normally for one object or Ctrl/Shift-click for multiple objects.

## 3. View and Camera

- Drag with the middle mouse button to orbit, drag with the right button to pan, and use the wheel to zoom.
- Iso, Top, Front, and Right switch to standard views.
- Perspective and Orthographic select the projection type.
- Fit frames every visible object.
- Number keys 0, 1, 3, and 7 select Iso, Front, Right, and Top. Number key 5 toggles the projection.

## 4. Projects and Files

- File > New Project (Ctrl+N) creates an empty project. Close Project returns to an empty workspace.
- Save Project (Ctrl+S) stores objects, geometry, History, Undo/Redo, selection, camera, grid, and snap state in an .s33d file. Subsequent saves use the current path. Save Project As… (Ctrl+Shift+S) chooses a new path and makes it the current project.
- Open Project (Ctrl+O) loads an .s33d project. New, Open, Close Project and application exit offer Save / Discard / Cancel when edits are unsaved. A cancelled or failed save keeps the current project. Selection/camera changes alone do not mark it modified; an asterisk beside its name indicates unsaved model changes.
- Save completes a valid Sketch/Draw 3D draft. Finish or cancel other active operations before saving; an unfinished stroke/edit is never silently omitted.
- Sketch and Draw 3D headings expand/collapse their tools. Collapsing retains the current draft; reopen the heading to continue, or use Exit Sketch / Exit Draw 3D to finish.
- Import supports STEP, IGES (.igs/.iges), STL, OBJ, PLY, GLB/glTF, and 3MF (.3mf). Newly imported objects are selected automatically. STEP/IGES imports run in the background; Cancel discards their result.
- SolidWorks: export the complete assembly or part as STEP (preferred) or IGES, then import it. S33D retains transferred B-rep geometry and assembly-relative placement, but does not recreate mates, configurations, the SolidWorks feature tree, or original part names/colors. Native .sldprt/.sldasm/.slddrw and Parasolid .x_t/.x_b require a separate converter and cannot be opened directly. A drawing is not a substitute for a 3D part/assembly. STL is mesh geometry, not the original CAD topology.
- Export supports GLB, OBJ, STL, PLY, and STEP. Choose **File > Export…**, then the format in the native save dialog (STEP, STL, OBJ, PLY or GLB for a selected object; SVG, DXF, PDF or EPS when a drawing is ready). File > Open/Import remembers the input directory; Save/Export independently remembers the output directory across restarts. Select one solid and choose a destination. Export runs in the background with progress and elapsed time; Cancel or Esc leaves the destination unchanged. The success message appears only after the complete file is published.
- Mesh-based threads and edge treatments export to STEP as validated planar faces, preserving bores and closed cavities. Small numerical contacts are conditioned within checked limits; open, substantially self-intersecting or oversized meshes report an error. Original exact surfaces retain their B-rep geometry. Failed exports preserve any existing destination file.

Model export suggests the current project name and defaults to STEP (`.step`). Select another format in the save dialog as needed.

## 5. Object Management

- The Visible checkbox controls whether an object is shown. Hidden objects are excluded from selection and Boolean operations.
- Right-click an object to Rename it, choose its Color, toggle Visible, or Delete it.
- In the color window, choose a color and press Apply Color. Cancel discards the change.
- Double-click an editable Sketch, Draw 3D object, or feature to reopen its parameters.

## 6. Sketch

- Select Sketch, then choose XY, YZ, ZX, a planar model surface, or a linear-equation plane.
- Line (L), Arc (A), Circle (C), Rectangle (R), and Polygon (G) create sketch geometry.
- Pick (V) selects sketch entities. Ctrl and Shift support multiple selection, with one Properties card for each selected entity.
- Key points show endpoints, midpoints, and centers. Magnetic snap can use both sketch geometry and reference-object key points.
- Done finishes the current drawing tool. Exit Sketch saves and leaves the sketch.
- Esc leaves Sketch and keeps completed entities. During an active drawing chain, the first Esc ends the chain.

### Trim

- In a new or edited sketch, select **Trim**. Hover an outline to preview the segment in red-orange; click to remove it. The tool stays active for successive cuts. **Done** returns to Pick; **Exit Sketch** saves and leaves.
- Cuts stop at intersections, line/arc endpoints and outline vertices. Trim an overhanging end or a middle segment without deleting the entire object. An isolated line or circle with no dividing intersections is removed entirely.
- Lines, circles and arcs use analytic intersections. A cut circle becomes an arc. Trimmed rectangles/polygons become separate line segments; freehand/text outlines are split at their stored outline vertices and intersections, and lose the original whole-object/text parameter editing. Other objects remain unchanged.
- Every click can be undone with **Ctrl+Z** and redone with **Ctrl+R**. Open outlines cannot be extruded until closed. A sketch driving a solid may temporarily fail dependency rebuilding after a cut; the last valid solid stays visible while you repair the outline.

### Freehand and Text

- **Pen (Freehand)**: each press-drag-release draws a new stroke, even when starting on or crossing existing strokes. Release finishes one stroke/one object/one Undo step and keeps Pen ready for the next stroke, without selecting the previous one. Return near the start to close the outline, or enable **Close each stroke**. Open strokes are lines, not extrudable regions. Switch to **Pick** to select and drag an existing outline; its farthest red bounding corner scales it uniformly.
- **Text**: click the workspace to place a transparent, dashed text box at the baseline origin. Type up to 64 characters here or in the sidebar; both update together immediately. All properties and actions stay in the sidebar. Choose Noto Sans SC (Chinese supported), Ubuntu Light, or Hack, and set **Font size**. Size is the font em height in the active length unit; visible letter height varies by font. The sidebar **Done** closes the text box and keeps the text object.
- Select a text object to edit its position, size, or clockwise rotation. Content/font changes rebuild the same object immediately. Outlines, including counters in O/B/8 and Chinese strokes, are stored in the project; font installation is not needed to reopen it. Embedded fonts retain their own redistribution licenses; see `THIRD_PARTY_LICENSES.md`.
- The dashed canvas box displays the actual modeling outlines at their final position, size and rotation, not a separate UI-font preview. Press **Enter** in the box to finish; **Shift+Enter** inserts a line break. Confirming a Chinese IME candidate does not finish the object.
- Exit Sketch and single-click any filled stroke to select all filled regions of that text object, then Extrude them together. Counters remain holes. Ctrl/Shift adds or removes a whole text group in the same sketch. A selected text object can also go directly from Sketch to Extrude. Grouping does not bridge the gaps between letters. Complex text can be split into shorter objects. This is simple outline text, not a rich-text/typesetting editor.

## 7. Sketch Move, Rotate, and Align

- Sketch Move supports multiple entities, numeric offsets, the current step size, and four instant direction buttons.
- Positive Sketch Rotate angles turn clockwise. The clockwise and counterclockwise 90-degree buttons act immediately.
- Custom Pivot specifies the rotation center with a point on the sketch plane.
- In Sketch Properties, choose the fixed point for a line, arc, rectangle, or polygon. The default is the first point placed, which is the center for a polygon. Move carries the point with the entity. Size, angle, and rotation edits keep it in place. Choose **Unlocked** to use a shared or custom pivot.
- Sketch Align requires two or more entities. Lock a reference, choose a world axis and Min, Center, or Max, then press Apply.
- Dashed Move and Rotate geometry is a pending preview. Apply commits it and Reset restores the preview values.

## 8. Draw 3D

- While creating in Sketch or Draw 3D, click another top-level tool to commit valid current changes and leave creation. An empty sketch, plane picker, or unplaced primitive exits without adding an object. Switching Draw 3D primitive types also commits the placed preview first. A failed Apply keeps the current session for correction. Sketch Move/Rotate numeric previews are applied when switching tools. The destination still requires its usual objects/closed regions; switching does not invent a selection.

- Draw 3D creates boxes, cylinders, spheres, cones, and tori.
- Click once to place the base point, move the pointer to size the object, and click again to confirm the initial size.
- Base position, width, depth, height, radius, and other exact parameters are available on the right.
- For a Sphere, the first click and **Center Position** fields set the sphere center, not its bottom. Changing radius keeps this center fixed. Numeric input and sizing use a lightweight display preview; Apply generates and validates the exact sphere, then Done commits it. Old projects keep their geometry unchanged. The Agent `primitive.create` position retains its legacy base-point convention: for a default world-oriented sphere centered at C, use position C − (0, radius, 0).
- After the second click, drag the preview or its blue base handle to move it in its base plane. Drag red handles to change width, depth, height or radius. The workspace readout and Properties update together, and repeated drags remain available before committing.
- Changes affect only the preview until **Apply**, then **Done**. Changing size or position after Apply requires Apply again; editing an existing primitive keeps its object ID.
- Apply updates the current object and Done creates it. While Draw 3D remains active, select a created primitive to edit it again.
- After Apply, Esc commits the current primitive and closes Draw 3D. Exit Draw 3D also commits a valid placed preview.

## 9. Extrude, Pocket, and Revolve

- Select a closed sketch region or a bounded planar surface. Press E for Extrude or P for Pocket. Pocket uses a selected solid or a unique visible solid linked to the profile. It can also infer a unique nearby solid based on the profile. Click the intended target in the viewport if several solids qualify.
- Extrude uses Depth and can be Symmetric.
- Pocket uses Depth or Through to cut completely through its target. A depth preview starts on the sketch plane and extends inward by the entered depth; a Through preview spans the target along the cutting axis.
- After a blind Pocket, its consumed sketch does not block hover or double-click selection of a matching planar floor. The opening rim and floor footprint must still match. Select the sketch in Objects to edit it; moved, edited, unrelated and coplanar regions keep their usual priority.
- Revolve requires an axis and uses clockwise-positive Angle values.
- Apply or Done commits the operation. Esc cancels it.

## 10. Move, Rotate, and Scale

- Move works on one or more objects. Drag the gizmo or enter world X, Y, and Z offsets.
- The six direction buttons move immediately by the current step size.
- Move To places the shared center of the selected group at an exact coordinate.
- Rotate supports world axes, straight model edges, and sketch lines. The +90-degree and -90-degree buttons rotate immediately.
- Scale by percent uses percentages. Scale to size targets exact bounding-box dimensions.
- Uniform Scale preserves proportions. Disable it to scale axes independently.

## 11. Align

- Align can start with zero or one selected object, then collect more objects from the Objects window or workspace.
- The first valid object becomes the locked reference by default; choose another Lock in the object list if needed.
- Choose Axis and Min, Center, or Max, inspect the preview, then press Apply.
- The object list scrolls when long, while Axis, Mode, and Apply remain easy to reach.

## 12. Boolean Operations

- Union accepts multiple objects. Selected operands share a preview color and merge when Apply is pressed.
- Subtract computes Original Object (A) minus Subtracting Object (B). Swap objects reverses these slots without applying the operation.
- Intersect uses the same two slots and keeps their common volume.
- Click either operand field to open a scrollable menu of visible solid objects (names and IDs). You can also pick in the viewport or Objects list. Choosing the other field's object moves it to the clicked field, clears the other field, and waits for a replacement; Apply requires both fields. Keep originals controls whether source objects remain visible.
- Boolean previews do not modify the document until Apply is pressed.

## 13. Mirror, Split, and Section View

- Mirror: select one or more objects, choose a mirror plane, then press Apply.
- Split: choose an object and cutting plane, then press Apply to create separate solids.
- Section View changes only the view. A non-intersecting or tangent plane keeps each object whole, on either side and with either Flip setting. Intersecting objects are sectioned normally. Drag the viewport arrow along the plane normal; the equation updates live in mm. Looking straight along the normal uses vertical dragging. Press Esc to leave; geometry and history stay intact.
- Planes can use XY, YZ, ZX, a planar model surface, or an ax+by+cz=d linear equation.
- Flip side displays the other half. Keep original controls whether the source object remains.

## 14. Chamfer and Fillet

- Start Chamfer/Fillet, then select an edge or continuous edge chain.
- Chamfer uses Distance and Fillet uses Radius.
- On a convex slanted edge, Radius is the tangent-circle radius, not the setback along either face. Sharper internal angles need a larger setback; reduce the radius if nearby geometry leaves too little space. Existing saved mesh results do not change automatically after a software update: re-edit the feature and Apply to regenerate it.
- Apply changes the solid immediately. Reapply different parameters on the same edge to update that feature. Applying on another edge keeps the previous operation and builds the next feature on its result, without requiring Done first.
- Done manually finishes the current edge and keeps the tool open. Exit Chamfer/Fillet applies the latest parameters, finishes the edge, and collapses the tool menus. Double-clicking a face or triple-clicking an object also exits and performs that selection.
- Select a feature again in Objects to edit its parameters. Deleting a middle feature rebuilds its later dependents.
- For a straight-walled polygonal hole, click a side of the opening and choose Apply. Continue around the rim using the same Distance to join the adjacent chamfers, including the last side back to the first. If the offset sides cross, reduce Distance. To rebuild a saved run of equal-width chamfers, select its last feature in Objects and apply the original Distance again.

## 15. Array, Copy, Paste, and Delete

- Array can enable X, Y, and Z simultaneously, with separate direction, Spacing, and Copies settings.
- Copies includes the original object.
- Copy stores the selected object and Paste creates successively offset copies.
- Delete or the Del key removes the current object selection.

## 16. Units, Step, and Snap

- Settings > Units offers millimeters, centimeters, meters, inches, and feet.
- Models and project files always store millimeters internally. Changing the display unit does not change the physical size.
- The status bar shows the active unit. Step is converted to the equivalent value in that unit.
- With Snap enabled, sketches and movement align to the step grid. Disable it for continuous coordinates.

## 17. Undo, Redo, and History

- Ctrl+Z or Edit > Undo reverses the last document change.
- Ctrl+R, Ctrl+Shift+Z, or Edit > Redo restores it.
- Undo/Redo restores document geometry and object selection. An active Sketch uses its own session timeline.
- History lists recent actions, but selection messages do not always represent undoable geometry changes.

## 18. Scientific Calculator

- On startup S33D reads `Plug-ins/*/plugin.json` beside the executable (workspace `Plug-ins` for Cargo debug/release builds). Only valid manifests for supported `builtin:<id>` entries appear as installed; removing a manifest removes its menu entry after restart. There is no arbitrary DLL/script loading. Unknown, invalid or duplicate manifests are reported in the Plug-ins menu. Distribute the Plug-ins folder with the executable; the working directory is not used for discovery.
- The Plug-ins menu aligns names, enable switches and Open actions to the left. Slide the switch right to enable, left to disable; choices are saved. Open also enables the selected plug-in. Enabled plug-ins appear in the compact toolbar below the menu bar; expand/collapse retains parameters. Restart after installing or removing manifests.

- Enable or open the calculator from Plug-ins.
- It supports arithmetic, powers, parentheses, Ans, pi, e, square root, trigonometric and inverse trigonometric functions, logarithms, absolute value, and exponentials.
- DEG and RAD select degree or radian input for trigonometric functions.
- Enter or = evaluates; C clears; the backspace button removes the last character.

### Generator and surface plug-ins

- Gear Generator, Tire Generator, Wheel Rim Generator, and Surface Texture are optional built-in tools enabled from Plug-ins.
- Tire Generator creates Highway Rib, Block, Chevron, or All-Terrain tires from outer diameter, rim diameter, section width, tread depth, and repeat count.
- Wheel Rim Generator creates Five Spoke, Six Spoke, Split Five Spoke, or Multi-Spoke rims with editable hub, center bore, and bolt-pattern dimensions.
- Select a planar face or a complete cylindrical wall, including the inside of a bore, to apply Surface Texture. It creates straight, left/right diagonal, or diamond knurling with editable pitch, width, depth, angle, and margin. On a cylinder, the spacing adjusts to fit a whole number of grooves around the circumference.
- Generated objects retain their parameters. Select one and reopen its plug-in to update it without changing its Object identity. Surface Texture keeps its source Object hidden as a non-destructive dependency.

## 19. Settings and Beginner Mode

- Settings controls Language, Font, Font Size, Appearance, Accent Color, Units, Getting Started, Beginner Mode, Agent Control, and Debug Logging. Agent Control includes its enable switch, port, connection test, registration, and unregistration in one submenu.
- Getting Started can change the appearance, language and default unit and can enable step-by-step guidance.
- Closing Beginner Guide disables that guidance and saves the choice.

## 19.1. Appearance and Tool Icons

Choose **Settings > Appearance** to switch between four saved styles:

- **Simple** (default): the existing compact, text-only toolbar. **Accent Color** still selects Blue, Teal or Orange for this style.
- **Precision Classic**: warm-white panels, a charcoal workspace and teal controls.
- **Graphite Studio**: dark panels and workspace with warm-orange controls.
- **Porcelain Workshop**: pale panels and workspace with indigo-blue controls.

The three new styles share purpose-drawn CAD vector icons alongside text labels and retain the existing tool order. Their toolbar is slightly wider and scrolls when necessary; Sketch and Draw 3D groups can collapse. Each preset has its own accent; the separate Accent Color setting applies to Simple.

Appearance changes take effect and save immediately, including after restarting S33D. They change only interface and workspace colors, not model geometry, object colors, selection or the camera.

## 20. Agent Control (Codex and Claude Code)

Agent Control lets a local AI coding agent or script inspect and modify the document open in S33D. The desktop app remains the only owner of the document, geometry kernel, selection, viewport, History, and Undo/Redo state; the bridge never maintains a separate model.

- Data flow: Codex or Claude Code uses MCP over standard input/output to run `s33d-agent.exe`; the bridge sends newline-delimited JSON to the S33D desktop app, using `127.0.0.1:53433` by default; S33D executes each accepted command on its UI thread through the existing Document and TruckKernel.
- Local scripts can use the bridge CLI instead of MCP. The loopback address is reachable only from the same computer by default.
- Protocol v2 enforces the 1 MiB frame limit during reading, 8 live connections and 16 queued requests. A queued command expires after 30 seconds and cannot execute later; started work returns `in_progress` and cannot be interrupted here. Geometry commands return `busy` while an interactive tool is unfinished.
- MCP writes require `idempotency_key` chosen before submission; CLI supports `--request-key`. Retry the same method, params and key after an ambiguous timeout. `request.cancel` with params `{"idempotency_key":"your-key"}` cancels queued writes. Replay is limited to 10 minutes after completion on the same running listener (restart/port change resets it), with 256 records and 8 MiB of input; full capacity returns `server_busy`. See AgentControl.md for cache limits and recovery.

Required runtime files and processes:

- `s33d-app.exe` is the desktop host and must be running with the project to edit open.
- `s33d-agent.exe` is the matching MCP/CLI bridge. Keep both executables from the same build.
- Closing S33D also ends its connected MCP bridges. After reopening S33D, restart the Agent if it does not reconnect automatically.
- Codex needs one `mcp_servers.s33d` entry in its `config.toml`; Claude Code needs one `s33d` entry in the project's `.mcp.json`.
- The example files are `agent-config/codex.toml.example` and `agent-config/claude-code.mcp.json.example`. They are setup templates, not additional runtime programs.
- No cloud account, API key, or geometry service is required by S33D Agent Control. The selected Agent application may have its own account requirements.
- Agent Control is enabled by default. The Enable local control switch under Settings > Agent Connect starts or stops the local listener immediately and saves the choice; turning it off does not change the model or History.
- The Port field accepts 1 through 65535 and defaults to 53433. Enter a value and press Apply Port. If it is occupied, S33D keeps the previous listener active and asks you to choose another port and try again.
- Test Connection applies a newly entered port when necessary and confirms that the S33D loopback listener accepts a local connection.
- Codex and Claude Code each have Register / Update and Unregister buttons. Register / Update tests the current port first, then changes only that Agent's user-level `s33d` MCP entry. Unregister removes only that entry; it does not delete S33D or any project.
- If an Agent CLI is missing, install or open that Agent and retry. If its configuration is protected, grant S33D permission to update the Agent configuration and retry.

Build and start both executables from the repository root:

```powershell
cargo build --release -p s33d-app --bin s33d-app --bin s33d-agent
target\release\s33d-app.exe
target\release\s33d-agent.exe status
```

The easiest Codex setup is Settings > Agent Connect > Codex > Register / Update. S33D discovers the bridge beside the running application, so each installation records its own correct absolute path. For manual setup, replace `<S33D_INSTALL_DIR>` with the installation directory on that computer:

```powershell
codex mcp add s33d -- "<S33D_INSTALL_DIR>\s33d-agent.exe" mcp
```

On macOS or Linux, use `codex mcp add s33d -- "<S33D_INSTALL_DIR>/s33d-agent" mcp`.

Alternatively, merge this entry into the Codex `config.toml`:

```toml
[mcp_servers.s33d]
command = '<ABSOLUTE_PATH_TO_S33D_AGENT>'
args = ["mcp"]
```

The single-quoted TOML value accepts Windows backslashes literally. In JSON configuration, either escape each Windows backslash as `\\` or use forward slashes.

The easiest Claude Code setup is Settings > Agent Connect > Claude Code > Register / Update. The equivalent user-level command is:

```powershell
claude mcp add --transport stdio s33d --scope user -- "<S33D_INSTALL_DIR>\s33d-agent.exe" mcp
```

On macOS or Linux, use `claude mcp add --transport stdio s33d --scope user -- "<S33D_INSTALL_DIR>/s33d-agent" mcp`.

For a project-level Claude Code setup, copy `agent-config/claude-code.mcp.json.example` to `.mcp.json` in the working project, or merge its `s33d` server entry into the existing file. Restart the Agent application after changing its MCP configuration.

For another MCP-capable Agent, click Other Agent Manual... to open this chapter in a separate window. Configure a standard stdio MCP server whose command is the absolute path to `s33d-agent.exe` installed beside S33D and whose only argument is `mcp`. The bridge follows the port saved in S33D automatically. On macOS or Linux, use `<S33D_INSTALL_DIR>/s33d-agent` instead.

The MCP bridge exposes six tools:

- `s33d_describe` reports the live protocol version, method catalog, parameter examples and units. Call it at the beginning of a session instead of guessing parameters.
- `s33d_state` reports the open document, selected objects, stable body IDs, features, sketch entities and closed regions, triangle counts, visibility, and world-space bounds.
- `s33d_execute` runs one catalog method with its parameter object. Useful methods include `primitive.create`, `sketch.create`, `feature.extrude`, `feature.revolve`, `body.transform`, `boolean.apply`, file/project commands, and History commands.

- `s33d_workflow` returns the built-in modeling guide for the requested workflow.
- `s33d_reference` lists optional project photos or returns one imported photo by asset ID.
- `s33d_render` returns a native model image for inspection and comparison.

New commands: `sketch.trim.preview` takes `sketch_id`, a local-plane UV `point:[u,v]` and positive `tolerance` in mm. It returns the removed segment, replacements, resulting regions and `preview_token` without editing anything. To commit, call `sketch.trim.apply` with the same inputs and that token. It retains the sketch body ID, rebuilds supported dependents and supports Undo/Redo. Changed inputs/sketches require a new preview; failed dependent rebuilding changes nothing. Preview does not validate dependents. Only committed Sketch objects in Idle mode are supported; composite outlines become segments and region indices must be re-queried afterward. `view.recenter` with `{}` centers visible objects and restores perspective without modifying geometry (Idle mode only).

Recommended Agent workflow:

- Start S33D and finish or cancel any interactive tool.
- Get `s33d_workflow` and `s33d_describe`, then read `document.summary` through `s33d_execute` before changing the model. Use `s33d_state` when full geometry detail is needed.
- Run one operation through `s33d_execute`, using the body IDs or sketch region indices returned by S33D.
- Requery the document summary and verify the feature type and bounds. Use `s33d_render` to inspect the native model image; `s33d_reference` is for optional photos stored in the project. Use `history.undo` if the result is wrong.
- All Agent Control lengths and coordinates are millimeters, even when the UI displays another unit. Angles are degrees.

A direct CLI example creates a 40 by 20 by 8 mm box:

```powershell
target\release\s33d-agent.exe call primitive.create '{"kind":"box","name":"Base","position":[0,0,0],"width":40,"depth":20,"height":8}'
```

Use `target\release\s33d-agent.exe describe` for the authoritative command catalog and `target\release\s33d-agent.exe call document.get` to inspect the model without an MCP client. If `status` cannot connect, turn on Enable local control in Settings > Agent Connect; the active address is shown below the switch. Without an explicit `--address` or `S33D_AGENT_ADDRESS`, the bridge reads the saved port from the platform's per-user S33D configuration (`%LOCALAPPDATA%\S33D` on Windows, `~/Library/Application Support/S33D` on macOS, or `$XDG_CONFIG_HOME/s33d` on Linux). A legacy working-directory settings file is migrated automatically. Set `S33D_AGENT_DISABLE=1` before launching S33D for an environment-level hard disable.

The protocol uses deterministic, ID-based construction and editing. Exact offset exposes current-source topology face indices; it does not provide general stable edge IDs for all operations. Visual fillet-edge selection remains a desktop operation unless listed by the live catalog.

## 21. Spatial curves and exact geometry

### 3D equation curves

Choose **Draw Equation**, a top-level tool alongside Sketch and Draw 3D; no sketch plane is needed. Enter x(t), y(t), z(t), or switch to R(t), theta(t), phi(t). Set a finite increasing t domain and the expression length unit (mm/inch). Angles use radians unless Degrees is checked. In spherical coordinates theta runs from +X toward +Y in XY, and phi starts at +Z; these are world axes, independent of camera orientation.

Example helix: `20*cos(t)`, `20*sin(t)`, `2*t`, domain 0 to 12.5663706144, radians, mm. Supported elementary functions include sin, cos, tan, asin, acos, atan, sqrt, exp, ln, log (base 10), abs, with pi, e, parentheses and + - * / ^. Use explicit multiplication. Expressions are limited to 256 bytes; sampling is bounded and approximate. Invalid coordinates, negative spherical radii and unresolved discontinuities fail preview; narrow the domain when necessary.

Inputs refresh the preview immediately. Apply preview recalculates it; Done or Exit Draw Equation commits and leaves the tool. Switching to another tool also commits valid input; invalid geometry or a failed dependent rebuild keeps the editor open. Cancel discards the draft. Select a result in Objects or click its line; double-click its Objects row or use Edit equation to edit without changing its ID. Equations, units and transforms survive save/reopen and Undo/Redo.

Coplanar equation lines participate in the same closed-region selection and highlight as Sketch: double-click an enclosed region, then Extrude or Pocket. A single enclosed region can also be extruded after selecting its curve in Objects. For example use `20*cos(t)`, `20*sin(t)`, `5`, domain 0 to 6.28318530718. Ordinary planar extrusion remains non-symmetric by default. Editing the equation rebuilds linked Extrude/Revolve features; invalid edits preserve the original document. Non-planar curves are not flattened and open endpoints are not forcibly joined. Curves themselves are not solids and are not mesh/STEP export geometry; save the project to retain their definitions.

### Shared planar regions and exact face extrusion

Visible equation curves on the same plane now participate in one shared region arrangement. Two concentric closed curves offer a ring and an inner disk; two open curves can form a region only where their actual segments close it. Double-click the desired region before Extrude/Pocket/Revolve. Hide other curves to exclude them. Non-coplanar curves are never projected into the group. Multi-curve Extrude/Revolve results retain a complete profile snapshot, not a live link to just one curve; later edits of those curves do not rebuild that result. Single-curve linked features retain their existing rebuild behavior. Agent clients must refresh the reported regions after curve edits or visibility changes and supply the desired `region_indices` to `feature.extrude`.

Direct Extrude of a complete planar face on an exact B-Rep object sweeps its original boundary wires, including curved edges and holes, then joins it to the source. This includes planar rims of split sphere shells. Display triangles are not substituted for those native boundaries. Symmetric planar Extrude uses the entered total depth, half on each side. This is different from local-normal thickening of a curved equation surface. Invalid/self-intersecting results still fail without replacing the source.

### Equation surfaces and Fill

In Draw Equation choose **Surface**. Enter x(u,v), y(u,v), z(u,v), or R/theta/phi with two finite increasing domains. Coordinates and angular conventions match Line mode. Subdivisions per axis ranges from 4 to 64; samples are interpolated as a smooth B-spline B-Rep, not a symbolic or error-bounded exact solution. Avoid discontinuities, folded/self-intersecting definitions and undersampling. Increasing subdivisions changes the interpolation and can cost more time.

For a saddle, use `u`, `v`, `0.02*(u^2-v^2)`, both domains -10 to 10. For a closed sphere use spherical mode: R=`10`, theta=`u`, phi=`v`, u=0..6.28318530718, v=0..3.14159265359, radians. **Fill** accepts only an already closed, valid positive-volume surface; it never adds caps to an open surface. Leave Fill off to create a Surface object.

Input changes request background previews. Apply preview recalculates; Done/Exit commits the ready result. A tool switch commits a ready valid draft; while computing or invalid, stay in the editor and try again after the preview succeeds. Cancel discards the draft and ignores late results. Switching Line/Surface replaces the uncommitted draft. Select the result and Edit equation (or double-click its Objects row) to keep its ID while changing parameters. Save/reopen and Undo/Redo preserve definitions and B-Rep. Surface objects support STEP export.

Select an unfilled equation Surface and choose **Extrude**. This follows local normals, not a fixed world direction. Symmetric is enabled only for this surface-extrusion entry: a total thickness of 2 mm adds 1 mm on each side. Uncheck it for one-sided normal thickening. Apply preview, inspect, then Done; Keep original retains the source. Impossible thickness/self-intersections fail without committing. The result is an offset feature with a frozen source snapshot; subsequent equation edits do not rebuild that result.

Agents use `surface.equation.preview/apply` with the live catalog's parameters; use `feature.offset.start` with `SymmetricThickenSurface`, positive total `distance`, and `faces: []`, then status/commit/cancel for surface extrusion. Native geometry can take time and cancellation discards results rather than forcibly stopping computation.

### Exact offset and shell

Import the original STEP to retain analytic/spline surfaces and topology. Select one imported object, then **Offset / Shell**. Choose Offset surface, Hollow solid, or Thicken open surface. For hollowing, select faces to remove in the viewport or the scrollable face list. Set thickness and tolerance, Apply preview, inspect, then Done. Negative thickness points inward for an outward-oriented solid. Keep original retains the source; clearing it hides rather than deletes the source. Cancel discards the preview, including a late background result.

Open surfaces appear as Surface objects. Successful shell/thickening results must be valid closed solids; impossible thickness, self-interference or invalid input produces an error without replacing the original. Arbitrary imported surfaces are accepted without a primitive whitelist, but success is not guaranteed for every model or thickness. STL/OBJ and old mesh-only projects cannot recover original smooth surfaces: re-import STEP when available. Results retain a frozen source snapshot, not a live upstream link. Project version 2 preserves B-Rep through save/reopen and Undo/Redo; older programs cannot open version 2.

Agent control adds curve.equation.preview/apply and feature.offset.start/status/commit/cancel. Consult the live catalog for parameters. Offset uses a returned job_id; ready means preview ready, not committed. Face indices are zero-based topology indices from the current body. Cancellation discards results but does not forcibly interrupt native computation.

## 22. Keyboard Shortcuts

- Ctrl+Z: Undo; Ctrl+R: Redo; Ctrl+A: Select All.
- Del or Backspace: Delete; Esc: Cancel or leave the current tool.
- S opens Sketch, E opens Extrude, P opens Pocket, and O opens Revolve. Q also opens Pocket.
- L: Line; A: Arc; C: Circle; R: Rectangle; G: Polygon; V: Pick.
- M: Move; T: Rotate; F: Fit View.

## 23. Troubleshooting

- Object not visible: check Visible in Objects, use Fit, and verify that Section View is not hiding it.
- Cannot select: confirm that the object is visible and that the active tool accepts its object type.
- Missing glyphs: confirm that suitable Windows fonts are installed, then restart S33D so font fallback reloads.
- Disabled action: select the requested region, surface, edge, or object. Press Esc if another tool is active.
- Boolean or Chamfer failure: check that objects intersect and that the parameter fits the local geometry, then try a smaller value.
- Agent cannot connect: keep S33D open, confirm `s33d-agent.exe status` succeeds, make sure the bridge and desktop use the same loopback address, and check that another S33D instance is not already using the port. If the port is occupied, choose another Settings port and press Apply Port again.
- Agent reports busy: finish or cancel the active Sketch, Draw 3D, Boolean, Mirror, Split, Section View, Extrude, Pocket, or Revolve operation, then retry the command.

## 24. 2D Engineering Drawings

Open **3-view Drawing** on the left. Its menu expands/collapses and is mutually exclusive with Sketch and Draw 3D. The sheet stays available when you collapse and reopen the menu; switching back to a modeling tool restores the 3D workspace.

1. The Drawing tool directly inherits the Objects list selection (including explicitly selected hidden objects). Select one or more 3D objects in Objects, then choose **Generate / Refresh**. With no selection, S33D projects all visible geometry. Until generation succeeds, the original 3D workspace and camera remain visible; no empty sheet replaces the model. The worker produces FRONT, TOP and RIGHT views using third-angle projection on an A4 landscape sheet, at one fitted scale. The fixed world directions are front: X/Y viewed from +Z, top: X/-Z viewed from +Y, right: -Z/Y viewed from +X. Rotating the camera does not change the drawing directions.
2. **Automatic dimensions / Auto dims** replaces annotations with overall sizes, verified cylinder/bore diameters, center spacing chains and distinct straight outline lengths/steps. Center marks identify circular features. Additional **Key dimensions** sheets list every source object's X/Y/Z bounds and world position, verified cylinder/bore diameter, axial length, axis base/direction, known chamfer/fillet/offset/thread parameters, and horizontal/vertical outline stations. Use the Sheet arrows in Properties to browse them. This is geometric measurement, not inferred design tolerances or manufacturing requirements; arbitrary meshes do not automatically become verified circular holes.
3. For **Manual dimensions / Manual dims**, choose Smart, Width / horizontal, Height / vertical, Length / aligned, Diameter Ø or Radius R. Select a highlighted edge, circle or arc, then click to place its dimension. Linear tools also accept two actual endpoints or circle centers followed by placement. A moving preview shows the measured value before placement. Aligned measures the distance in that projection, not the spatial length of an oblique 3D edge. Esc cancels unfinished points; another Esc with no pending points leaves the drawing.
4. Undo dimension, Ctrl+Z and Edit > Undo undo drawing annotations while the sheet is active. Ctrl+R, Shift+Ctrl+Z and Edit > Redo restore them. Clear dimensions and automatic dimensions are undoable as single steps; the annotation history holds 200 steps. These operations do not change the source model.
5. Edit the Drawing title and choose **Export** in the expanded left Drawing menu or Properties, or **File > Export…**. Choose SVG, DXF, EPS or PDF in the save dialog; the Drawing panel also retains its format submenu. SVG/DXF/EPS write the main file and all `-dimensions-N` companion sheets beside it; keep these files together. PDF writes all sheets into one multipage document with an embedded Chinese font. All formats retain vector geometry, A4 page layout and dimensional labels in the current Settings unit. Print at 100% to preserve the stated fitted scale.
6. The source-colored **Selection · 3D preview** in Properties updates when you change the Objects selection. It is an on-screen reference, separate from the generated drawing snapshot, and is not printed in exported sheets. Click Generate / Refresh to make the drawing match a new selection. The complete mesh is depth-tested and fitted inside the preview; this does not change the model or projected lines.

This implementation uses tessellated feature/silhouette edges with triangle depth tests at sampled points on each edge, rather than exact native hidden-line removal. Smooth curves are polygonal approximations and very small or nearly coincident details can be missed. Limit a sheet to 200,000 input triangles; select fewer objects for large assemblies. Dense dimensions are continued on additional sheets rather than discarded. Review dimensions and add manual annotations as needed before production.

Drawing snapshots, dimensions and placement, text overrides, title-block fields and component notes are saved in .s33d and restored on reopen. Drawing-only edits trigger the unsaved-changes prompt. Wait for projection generation to finish before saving. Older projects without drawing data open with an empty drawing. Annotation Undo/Redo history, unfinished picks, active editors and background jobs are not saved. Refresh manually after model changes; drawings are not associative. Refresh replaces annotations and resets their history. New Project clears the drawing; Open Project restores its saved snapshot. Exact HLR and geometric tolerances are not implemented.

### Feature dimensions and drawing format

The 3-view Drawing preview fits the full selected geometry and uses depth-tested faces. Generate / Refresh captures a drawing snapshot. Hidden edges can be shown as dashed lines, including closed internal contours; these are tessellated projections, not an exact section view.

Enable Manual dimensions or choose a dimension tool. Smart dimension recognizes an entire line, a circle, or a circular arc and highlights the feature before selection. Click the feature, then click to place the dimension. Width / horizontal, Height / vertical and Length / aligned also accept two endpoints or circle centers. Diameter Ø and Radius R require a recognized circular feature; arbitrary two-point distances are not treated as diameters. Esc cancels the pending dimension. Annotation Undo / Redo remains separate from modeling history.

Auto dimensions provide located hole/cylinder IDs on the views. The upper-right feature table lists diameter, depth/axial length and center coordinates relative to each view’s lower-left bounding corner; overflow continues on the dimension sheets. Inclined bores and cylinders have selectable elliptical rim projections. Smart, Diameter and Radius tools use the true major-axis size, rather than the foreshortened minor axis; automatic dimensions and located callouts cover these inclined features. Circular arcs use radius measurements. Hidden contour coordinates and lengths remain in the dimension schedule. Mesh-fitted circles/arcs are tessellation measurements, without inferred tolerances.

Newly generated threads retain surface origin/axis plus their parameters, so thread callouts can show placement, diameter, pitch, length and start offset. Older thread records did not store the applied surface location: they retain their parameter schedule but need thread regeneration for located callouts. Geometry modified after threading invalidates the saved location rather than displaying a misleading annotation.

Expand Title block to edit drawing number, revision, material, drawn by, checked by and date. The bottom-right table shows these fields with title, A4 size, scale, units and page number. All exports use the same title block and hidden line style; additional schedule sheets include the same identification fields. Drawing settings and annotations are saved with .s33d; exports are for exchange and printing.

### Editing a generated drawing

Choose **Edit drawing text / dimensions** when a dimension tool is active. Click any printed text or a blank title-block cell, edit **Selected text** in Properties, and press **Apply text** or Enter. The editor is brought into view automatically. Title, drawing number, revision, material, author/checker and date update their shared title-block fields on all sheets. Other text, including feature-table and schedule rows, can be overridden on the current sheet; **Reset text** restores the generated wording. Text replacement changes the annotation only, never the source geometry or its measured value. Overrides are tied to the generated text and page, so changing the unit/language does not reinterpret an old numeric override in a new unit.

Drag ordinary text, feature labels or title-block text to reposition it; feature leaders retain their original anchors. Drag a dimension's text to reposition its dimension line. Model projection lines remain locked in S33D. Select dimension text and choose **Delete dimension** to remove that annotation. Text changes, title-block fields, placement and deletion participate in drawing Undo/Redo. Every export includes the current edited text on every sheet. Refresh creates a new snapshot and clears generated-text overrides, text offsets and dimensions; title-block fields remain. These edits are saved with .s33d; annotation Undo/Redo history is not saved.


### 24.4 Export formats

**Export** is available in the expanded Drawing menu and Properties; **File > Export…** offers the drawing formats in the native save dialog. The Drawing panel keeps its format submenu. Use the matching file extension; mismatched extensions are rejected.

- **SVG:** one editable vector file per sheet, including XML-escaped edited text.
- **EPS:** one EPSF-3.0 vector file per sheet, including all companion schedules, edited text, positions, rotation and hidden-line dashes. The required TrueType glyphs are embedded; numeric text uses text operators and Chinese retains Unicode glyph names. Model projection lines are locked in S33D. Text editability and locks after external import depend on the editor; EPS itself provides no access-control lock.
- **DXF:** ASCII R2000 exchange drawing with editable LINE/TEXT entities and separate OUTLINE, ANNOTATION and HIDDEN layers. Hidden lines retain their dash pattern. Coordinates use **paper-space millimetres at the stated fitted scale**, including when dimension labels use inches; this is not a 1:1 model-space export. Dimensions are exploded lines/text, not associative DIMENSION constraints. Text uses Unicode escapes and the Noto Sans SC style; a receiving CAD needs a suitable Chinese font to display Chinese labels.
- **PDF:** one A4 landscape multipage document containing the main drawing and every dimension schedule. Lines remain vector paths; the bundled OFL-licensed Noto Sans SC font is embedded for portable, searchable Chinese text. Print at 100%, without fit-to-page, to preserve the stated scale.

**DWG is not built in.** The menu explains that a separately licensed writer is required. Export DXF and use your licensed CAD software to save as DWG. S33D does not bundle ODA SDKs/converters or link LibreDWG. LibreDWG is GPL-3.0-or-later; ODA SDK distribution requires a subscription and its terms. This is a distribution choice, not a claim that the DWG format itself is unlawful. See [LibreDWG](https://github.com/LibreDWG/libredwg) and [ODA licensing](https://www.opendesign.com/pricing).

Export includes current title-block fields and text overrides. It does not change geometry, provide exact HLR, infer design tolerances, or make the on-screen 3D selection inset part of the printed drawing. Drawings and their edits are saved with .s33d; exports are for exchange and printing.

### Multi-hole sketch selection (2026-11-09)

After exiting Sketch, double-click the required material region; its holes remain open. Switching or adding another object in Objects, or editing the profile or plane, clears the old region selection. Pick the region again. A profile with only one closed region can still be extruded directly.

Intersecting contours divide regions at their actual intersections. Dangling branches and lines that only connect separate closed loops do not become material boundaries; the original lines remain in the sketch. Boundaries that genuinely divide material still participate in selection.

If overlapping, self-intersecting, or touching boundaries fail validation, S33D keeps the outline and shows a warning. Properties retains the error after the brief notification disappears; expand **Region validation details** for the cause. Extrude, Pocket, Revolve and text/gear/thread caps stop that generation rather than commit missing faces. Repair the outline and select it again.

## Startup, background loading and cylinder diameter editing (2026-11-10)

A borderless engineering splash starts with a preferred size of 768×1024 and scales down proportionally to fit the current display work area. The workspace uses a preferred 1440×900 client size, limited by the work area, window borders and current DPI; smaller displays can use a minimum below 900×600 when necessary. The workspace restores normal window borders and resizing. Fonts, preferences and the embedded image load off the UI thread. The reference image keeps its full aspect ratio and stays visible for at least two seconds. Startup file activation extends this time until loading finishes or is cancelled. Background preparation and loading continue during the minimum display time.

Project opening and mesh/STEP/IGES imports use cancellable background work. Project and STEP file reads show actual byte progress; geometry restoration, CAD reconstruction and mesh parsing use stage text and a spinner when a reliable percentage is unavailable. Cancellation or failure never commits a worker result. An indivisible native call may continue after cancellation, but its result is discarded. Wait or cancel before another operation; repeated clicks do not start duplicate jobs.

Double-click a recognized complete cylindrical side or bore wall, change Diameter in Properties, and press Apply. Values follow the current display unit; geometry remains in millimetres. Inclined axes are supported. Parametric cylinders retain their primitive feature. Sketch Extrude/Pocket solids with certified complete circular walls are also supported: their faceted solid is validated and converted before editing, accounting for the actual polygon chords. The result is an exact direct-edit solid; the original shared sketch is unchanged. Other exact solids retain exact geometry and become direct-edit results. Body ID and material are preserved, with one-step Undo/Redo. Active downstream features can require source-parameter editing; already consumed Pocket mouth sketches retain their provenance. Edits that split a solid, interfere with another cylindrical wall or lose the complete wall are rejected. Raw imported mesh cylinders remain read-only.

Expanding Thread generator gives parameters two columns, collapses view controls and reduces History. The parameter area fills the visible space after measuring the footer controls, scrolling independently when needed. Validation warnings, Correct and Generate remain below the parameter area. Correct according to nominal diameter resizes the actual bore / blank while retaining the specified nominal diameter and all thread and print parameters. The bore diameter is nominal diameter minus twice the tooth depth; an external blank uses nominal diameter plus thread length times diameter taper. The verified resized wall remains selected so Generate can use the requested nominal size. Correction is one Undo step; generation is a separate step. Parametric cylinders, certified Sketch Extrude/Pocket walls and exact solids are supported; raw imported meshes remain read-only. Faceted conversion must pass solid validation and currently permits at most 100,000 triangles. Invalid parameters, interference or dependent features can prevent correction without changing the document.

For a sketch-extruded bore: double-click its cylindrical side wall, expand Thread generator, set the requested nominal diameter, tooth depth and length, press Correct according to nominal diameter when the warning indicates a size mismatch, then Generate after validation succeeds. The printing suggest action only updates parameters; finish any printing suggestion and parameter edits before Correct. Select the bore wall rather than its sketch mouth region. The bundled ThreadPrinting.md describes the workflow and limits.

After threading only part of a wall, each complete uncut cylindrical band remains independently selectable by double-click for another thread operation. A middle cut retains both end bands. Start offset is relative to the currently selected remaining band. These bands survive saving, reopening and Undo/Redo. Existing projects with saved thread provenance recover uncut bands after verification against the actual mesh.

## Small windows and display scaling (2026-11-11)

Getting Started scrolls its instructions while keeping its close control, “Don't show this again” and Got it accessible. Gear, Surface Texture, Tire and Wheel Rim generators scroll their parameter and preview area; validation and Generate/Apply remain below it. Scroll inside the parameter area to reach an expanded Position section. The Thread generator also scrolls parameters separately from warnings, Correct and Generate; saving a preset opens its own window so it does not consume the properties footer. Resizing a window or changing the font size recomputes the space available for these controls. Texture preview lines stay inside their preview box.

Windows placement uses the current monitor's work area and DPI, including the taskbar and normal window borders, and corrects the actual size returned by the OS. It does not change system display settings. These changes have automated layout and input coverage; physical mixed-DPI monitor moves, 4K font clarity and GPU rendering still require native display validation.


### Photo reconstruction with an existing Agent

The Agent interprets your photos and calls S33D to model, then compares native model images with the references and corrects the result.

1. Open S33D. In Settings > Agent Connect, enable local control and select Test Connection.
2. Under Codex select Register / Update, then reopen Codex. Repeat registration if the installation location changes.
3. In a local Codex chat on the same computer, attach photos and specify dimensions, modeling requirements and the editable `.s33d` save destination.
4. The Agent builds, compares, corrects and saves the model. Continue changes in the same chat.

Importing photos into S33D is optional project reference storage or an alternate delivery route. Copy modeling prompt also works with an empty reference library. No manual skill, Python or development checkout is required. See [Photo to model](PhotoToModel.md) for full steps, an example, saving and import limits.


### Viewport guides and recent projects (2026-11-19)

The switch at the top right of the 3D workspace shows or hides the auxiliary grid and RGB reference axes together. The View menu uses the same switch. Settings > Re-enable all hidden tips also restores both guides. This changes the display only; model geometry, snapping and the orientation gizmo are unaffected. The existing grid visibility is saved with the project.

File > Open Recent lists up to ten recently opened or successfully saved projects, newest first. Hover over a project name to see its full path. Select it to open directly; unsaved changes still offer Save, Discard or Cancel. The list is kept in your local preferences across restarts. Failed or missing projects leave the current document and list intact; locate a moved file through Open Project to update its entry.


The title bar and Help > About S33D show the build identifier. About also shows UTC build time and installation channel, with Copy version information for support. The installer filename carries the same identifier.
