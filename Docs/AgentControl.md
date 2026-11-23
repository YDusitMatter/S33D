# S33D Agent Control

Objects-list Shift selection is a desktop gesture, not a new MCP command. Agents continue to use `selection.set` with explicit stable `body_ids` from `document.get`; never infer a range from numeric ID order.

`document.get` also returns logical `groups`, each with `id`, `name` and `members` (stable body IDs, including hidden members). Groups do not fuse geometry. `body.get`/document body entries report a generated fastener's stored feature recipe. Group/Ungroup, center-axis alignment and Screw/Nut Generator creation/update currently use the desktop UI; no new write commands for these are advertised. Query `system.describe` before executing a method.

`document.get` 同时返回逻辑 `groups`，每组包含 `id`、`name`、`members`（稳定 Body ID，可能包含隐藏对象）；编组不会融合几何。对象信息可读取紧固件保存的特征配方。编组/解组、中心轴线对齐及螺钉/螺帽配方生成或更新目前通过桌面界面操作，没有对应的新写入命令；执行前以 `system.describe` 为准。

The desktop Objects context menu captures the selected group when more than one object is selected, including when right-clicking another row. Only batch visibility, color and deletion appear; Rename is single-object only. The color dialog retains the captured IDs, and each batch is one Undo step. This does not introduce an MCP batch command or change existing agent command arguments.

Objects-list selection includes hidden rows (ordinary click, Ctrl toggle and Shift range), without changing their visibility. Workspace picking remains limited to visible geometry. Batch Visible can show a mixed visible/hidden selection; this desktop change does not alter agent selection arguments or tool operand validation.

## Desktop display updates (2026-11-11)

Generator parameters and Getting Started instructions scroll within short windows; their footer controls remain accessible. Thread **save as preset** now opens a separate desktop window. Windows startup/workspace placement fits the current work area and DPI. These are desktop layout changes; MCP methods, arguments, units and geometry semantics are unchanged. For desktop automation, locate current controls after scrolling or resizing rather than reusing screen coordinates.

生成器参数与首次指引正文可在短窗口内滚动，底部操作保持可用；螺纹 **save as preset** 改为独立窗口。Windows 启动页/工作区按当前工作区与 DPI 适配。本次不新增 MCP 方法，也不改变参数、单位及几何语义；桌面自动化在滚动或改变尺寸后应重新定位控件。

## Desktop interaction updates (2026-11-08)

Workspace single-click selects an edge, double-click selects a face/closed region, and triple-click selects the whole object. In Draw 3D, single-click selects and edits a body. Nonconflicting tool exits triggered by double/triple-click first name the tool and ask for confirmation: Confirm applies and finishes the current valid state, collapses tool menus and continues the original selection; Cancel retains the tool and draft. “Don't show this again” is saved only on Confirm. Settings > Re-enable all hidden tips restores this confirmation and Getting Started tips. Point construction keeps its original clicks. Explicit Exit/Cancel buttons do not add this dialog.

工作区连击退出先确认；确定后执行 Applied and Done、收起菜单并延续选面/选对象，取消保留工具及参数。“不再显示”仅随确定保存；设置可恢复所有隐藏提示。阵列及相对变换不重复应用，参数无效时保留编辑器，后台运算完成后再退出。普通空闲选择、逐点构造和明确退出/取消按钮不增加该提示。

While this confirmation is pending, desktop-dispatched requests except `system.ping`, `system.describe` and `workflow.get` return `busy`, including state queries. Transport-level `request.cancel` retains its existing queue-cancellation behavior. Resolve the dialog in the desktop and retry the intended request. The bridge advertises these desktop behaviors through `system.describe`; there are no new tool-exit, settings or tip-management MCP commands. Desktop-only Group/Ungroup buttons use the existing document group behavior.

## Solid threads (2026-10-29)

Desktop shortcut: double-click a supported cylinder/bore wall to select the complete lateral surface. The right-hand Thread generator entry appears only for that recognized selection; click to expand geometry, compatible presets and editable parameters. Right-click Generate thread also opens the draft; Generate thread performs the cut. Editing parameters switches the preset to Custom. The desktop print suggestion defaults to FDM 0.4 mm nozzle / 0.2 mm layers, with editable nozzle/layers and a resin option. The ... button next to suggest for 3D printing opens independent printing settings. Close the window and click suggest to update thread parameters; settings changes alone do not change the draft. FDM suggestions use complementary 90-degree truncated trapezoids (depth 0.25*pitch, crest/root flats pitch/4); resin uses 60 degrees and depth 0.433013*pitch. These are calibration starting points, not certified tolerances. Personal presets persist all settings in the user configuration directory. These are desktop gestures, not new MCP methods. A visible on-body sketch matching a bore's circular opening does not block hover or double-click selection of the inner wall. To edit that sketch, select it in Objects and choose Edit Sketch. A consumed blind-Pocket profile also yields hover/double-click selection to its matching planar floor when the real opening rim and recessed footprint still coincide. This applies to circular and polygonal profiles, including saved/reopened projects; unrelated, moved, edited and coplanar regions keep their existing selection priority. Select the sketch in Objects to edit it. Pocket depth previews start at the sketch plane without displaying the Boolean robustness extension; Through previews span the target axial bounds. Planar caps and adjacent holes remain separate; agents continue using explicit targets below. No MCP methods or parameter contracts change.

1. Call `thread.presets` to obtain built-in metric, UNC/UNF, BSPP and BSPT starting parameters. The old 3D-print preset has been removed; this method does not return desktop print suggestions or private user presets. Each preset supplies `external` and `internal` variants. Parameter lengths remain millimeters even in inch UI mode.
2. Call `thread.targets` with `{"body_ids":[1,2]}`. Keep the returned target objects intact, including `face_id`, origin, axis, radius, length and internal flag. Select the required surfaces from this array. Complete STEP cylinder/bore walls include geometrically recognized B-splines and connected split faces (0.0001 mm fit tolerance, two end-ring boundary validation). `face_id` is the stable representative face of a combined wall, not necessarily its only native face. Screw shanks, heads and shoulders remain separate when their radius changes. Recognized Draw 3D/simple cylinder features are also supported; faceted STL cylinder/bore bands are supported only after geometric certification (closed end rings and full-wall coverage, radial error at most 1% of radius and 0.02 mm). Mesh targets use synthetic IDs; incomplete arcs and interrupted walls are rejected.
3. Call `thread.apply` with `{"targets":[...],"params":{...}}`. A minimal M6 external example is `{"diameter":6,"pitch":1,"length":8,"offset":0,"depth":0.61343,"angle_deg":60,"clearance":0,"radial_fit":0,"left_hand":false,"taper":0,"profile":"truncated_v","flat_fraction":0.16666666666666666}`. Profiles are `truncated_v` (adjustable included angle and root flat/pitch ratio) or `rounded_55` (fixed Whitworth 55 degrees and depth 0.640327 times pitch). V-profile preset defaults use a root-flat/pitch ratio of 1/6 for external threads and 1/8 for internal threads. The rounded profile samples rounded crests and roots. Presets are geometry starting points, without fit-class/gauge certification.
4. Up to 32 targets share one parameter set. External blank radius must match `diameter/2 + length*taper/2`; internal cylindrical bore radius must match `diameter/2-depth`. Mismatched targets fail the entire batch. Requery before the next batch. Params stay available in the desktop panel for consecutive application. After a partial cut, only the processed axial intervals are excluded: each complete uncut band is returned as an independent target, including both ends of a middle cut. `offset` is measured from that returned band's `origin`, not the original wall's origin. Never reuse a target captured before the cut. Remaining bands survive save/reopen and Undo/Redo. Earlier saved thread records can recover omitted bands only when the stored mesh signature matches and the actual remaining wall passes coverage checks; geometry changes invalidate saved metadata. Provenance-backed thread remnants allow radial chord error up to 1% of radius with wall-orientation checks; the separate faceted-import certification above retains its 0.02 mm cap.

Desktop updates (2026-11-10): the two-column parameter area expands into the available height while keeping Warning, Correct according to nominal diameter and Generate outside its scroll area. Correct resizes the actual bore / blank to the requested thread, retaining nominal diameter and all thread / print parameters. Required bore diameter is `diameter - 2*depth`; external blank diameter is `diameter + length*taper`. The verified resized wall stays selected for Generate. Correction and generation are separate Undo steps; failure/cancellation leaves the document unchanged. Parametric cylinders, exact solids and certified Sketch Extrude/Pocket walls are supported. Faceted sketch solids are validated and converted to exact solids for the edit (100,000-triangle input limit); their shared source sketches stay unchanged. Consumed Pocket source metadata is retained. Interference, invalid topology or downstream dependencies can reject correction; raw imported meshes remain read-only. Double-click an uncut remaining band to thread it again. Recognized complete native/precise cylindrical walls also expose direct diameter editing. These controls do not add MCP methods; `thread.apply` continues to reject mismatched geometry without automatically resizing it.

Desktop workflow for a sketch-extruded bore: select the bore side wall, set the requested nominal diameter/depth/length, finish any printing suggestion, use Correct if the size mismatches, then Generate after validation. The printing suggestion changes parameters only; Correct changes geometry. Agent clients must requery `thread.targets` after desktop correction or any cut. See the bundled [printing and correction guide](ThreadPrinting.md) for the desktop sequence; there is no MCP Correct command.

Successful application replaces each source geometry with a **sampled closed mesh**, retaining object ID/name/color and a single Undo/Redo step for the whole batch. Native analytic faces on that object are no longer retained. No cosmetic-thread rendering, live preview, parametric history rebuild, tolerance class, thread entry chamfer or thread gauge check is implied. Existing source files are not overwritten automatically. Preserve a project copy when analytic geometry is needed downstream.


`radial_fit` is an optional per-part radial mating allowance in mm, default zero, in the range `0..pitch/4`. It shifts the complete external profile inward and the internal profile outward, including their crest envelopes. Two parts with the same nominal diameter, pitch, hand and allowance c have approximately 2c combined radial allowance. `clearance` retains its earlier meaning of additional groove depth and is not a substitute for this profile shift. Required blank/bore dimensions are still based on the nominal dimensions below; invalid targets and dimensions are rejected. Print suggestions and personal-preset management remain desktop-only; see [Thread printing notes](ThreadPrinting.md).

`length` must exceed pitch and fit within the selected cylindrical axial interval after `offset`. The requested `length/pitch` is limited to 125 turns; pitch must be at least 0.2 mm. Sampling uses 96 steps per turn. The tool extends one turn beyond each requested end, then is clipped to the explicit interval from `offset` to `offset+length`; the two internal overrun turns do not count against the 125-turn request limit. A thread that reaches a physical end face has grooves open through that end, while an offset or shorter length still produces a partial thread confined to the requested interval. This does not automatically add a production runout or entry chamfer. BSPP has zero taper. BSPT uses a diameter taper of 1:16 (`0.0625`); the specified diameter is at `offset`, **not the pipe standard gauge plane**, so gauge length/allowance must be set independently for a production part. Error replies use `thread_error`; no part of a failing batch is committed.

Preset dimension references: [REMS thread tables](https://www.rems.de/dl/10_cat/eng-SVN/sections/Gewindetabellen-eng-SVN.pdf) and [BAER BSPP reference](https://www.gewindebohrer.de/en/service/thread-standards/bspp-parallel-whitworth-pipe-thread-gb). These support the nominal sizes, pitch, included angles and taper; they do not certify this sampled implementation.

S33D Agent Control lets Codex, Claude Code, and ordinary local scripts inspect and modify the model that is open in the S33D desktop app. The desktop app remains the single owner of the document, geometry kernel, undo/redo history, selection, and viewport state.

## How it works

```text
Codex / Claude Code ── MCP stdio ── s33d-agent.exe
                                           │
Local scripts ─────── JSON CLI ────────────┤
                                           ▼
                                  127.0.0.1:53433
                                           │
                                           ▼
                                  running S33D desktop app
                                           │
                                  Document + TruckKernel
```

The loopback service is only reachable from the same computer. Protocol v2 uses newline-delimited JSON. Each frame is limited to 1 MiB during reading (including a missing-newline stream), with a 30-second absolute frame-read deadline. There are at most 8 live connections and 16 queued UI requests; excess connections are closed, and a full queue returns `server_busy` without admission. Agent Control is enabled by default and can be started or stopped immediately with **Settings > Agent Connect**; the choice is saved for the next launch. The same Settings group accepts a port from 1 to 65535, defaults to 53433, tests the active connection, and provides one-click registration for Codex and Claude Code. Set `S33D_AGENT_DISABLE=1` before starting S33D for an environment-level hard disable.

All Agent Control coordinates and lengths are **millimeters**, regardless of the unit currently selected in the UI. Angles are degrees. Body IDs returned by S33D are the stable references for later commands.

## File and process dependencies

Runtime requires two executables from the same build:

- `s33d-app.exe` owns the open document and executes every modeling command. It must remain running.
- `s33d-agent.exe` is the MCP/CLI bridge. Codex, Claude Code, or a local script starts or calls this file; it connects to the desktop host but does not load geometry itself.

The files under `agent-config/` are optional configuration templates. Codex reads an `mcp_servers.s33d` entry from its user `config.toml`; Claude Code supports a user-scoped `s33d` MCP entry as well as project `.mcp.json` files. S33D Agent Control itself does not require a cloud API key, online geometry service, database, or a second project file.

The source dependency chain is deliberately one-way:

- `crates/app/src/agent_protocol.rs` defines the shared JSON protocol, loopback server, client, limits, and typed parameters.
- `crates/app/src/app.rs` receives validated requests on the UI thread and dispatches them into the existing `Document` and `TruckKernel` operations.
- `crates/app/src/bin/s33d-agent.rs` translates MCP or CLI calls into protocol requests. It has no independent document state.
- `crates/app/src/main.rs` includes the desktop-side protocol module; the workspace build produces both binaries from the `s33d-app` crate.

The bridge and desktop protocol versions should match, so distribute or update the two executables together.

## Build and start

```powershell
cargo build --release -p s33d-app --bin s33d-app --bin s33d-agent
target\release\s33d-app.exe
```

Keep the S33D desktop app open while an agent is working. Check the connection from another terminal:

```powershell
target\release\s33d-agent.exe status
target\release\s33d-agent.exe describe
```

If `status` cannot connect, open **Settings > Agent Connect** and enable **Enable local control**. The Settings menu displays the active loopback address. Enter a different port and press **Apply Port** to switch, then press **Test Connection**. S33D binds the new port before releasing the old one; if the port is occupied, it keeps the previous listener and asks you to choose another port and try again. Turning the switch off immediately closes the listener; existing model data, History, and Undo/Redo are unaffected.

Unless `--address` or `S33D_AGENT_ADDRESS` explicitly overrides it, the bridge reads the saved port from the platform's per-user S33D configuration: `%LOCALAPPDATA%\S33D\ui_state.json` on Windows, `~/Library/Application Support/S33D/ui_state.json` on macOS, or `$XDG_CONFIG_HOME/s33d/ui_state.json` (falling back to `~/.config/s33d/ui_state.json`) on Linux. S33D migrates a legacy working-directory file on first launch. One-click registrations therefore use only the `mcp` argument and automatically follow later port changes.

## Codex setup

The normal setup is **Settings > Agent Connect > Codex > Register / Update**. S33D tests the current port and uses the Codex CLI to add or update only the `s33d` MCP entry. Restart or reopen Codex afterward. If Windows blocks the Codex configuration, grant S33D access to the user configuration and retry.

The equivalent manual command is shown below. Replace `<S33D_INSTALL_DIR>` with the installation directory on the computer being configured; the one-click action performs this substitution automatically.

```powershell
# Windows
codex mcp add s33d -- "<S33D_INSTALL_DIR>\s33d-agent.exe" mcp

# macOS / Linux
codex mcp add s33d -- "<S33D_INSTALL_DIR>/s33d-agent" mcp
```

Alternatively, copy the table in [`agent-config/codex.toml.example`](../agent-config/codex.toml.example) into your Codex `config.toml`. Restart Codex, then ask it to call `s33d_describe`, followed by `s33d_state`.
The template uses a TOML literal string so Windows backslashes need no escaping. In JSON configuration, escape Windows backslashes as `\\` or use forward slashes.

## Claude Code setup

The normal setup is **Settings > Agent Connect > Claude Code > Register / Update**. S33D tests the current port and creates or updates the user-scoped `s33d` entry through the Claude Code CLI. If the CLI is missing, install Claude Code first. If its configuration is protected, grant S33D access and retry.

The equivalent manual command is:

```powershell
# Windows
claude mcp add --transport stdio s33d --scope user -- "<S33D_INSTALL_DIR>\s33d-agent.exe" mcp

# macOS / Linux
claude mcp add --transport stdio s33d --scope user -- "<S33D_INSTALL_DIR>/s33d-agent" mcp
```

For a project-scoped setup instead, copy [`agent-config/claude-code.mcp.json.example`](../agent-config/claude-code.mcp.json.example) to `.mcp.json` in the project where Claude Code runs, or merge its `s33d` entry into an existing `.mcp.json`. Restart Claude Code and verify that all six S33D tools are visible.

## Other Agents and removing registration

Choose **Other Agent Manual...** in the same Settings group to open this chapter in a separate, scrollable window. For any Agent that supports a standard stdio MCP server, use the absolute path to the `s33d-agent` executable installed beside S33D as the command, with this argument:

```text
mcp
```

The bridge reads the port currently saved by S33D. Keep the desktop application open, press **Test Connection**, then restart the Agent after changing its configuration. `127.0.0.1` is an intentionally portable loopback address: on every computer it means that same local computer and does not expose S33D to the network.

With the matching updated executables, each MCP bridge connects to the desktop's lifetime when S33D becomes available. Closing S33D ends those bridge processes even if the Agent still has their input open. Reopening S33D requires a new MCP connection. Restart the Agent if it does not reconnect automatically. Changing the port or temporarily disabling local control leaves the existing MCP session open. Before the first desktop connection, the bridge can still provide the offline modeling guide.

**Unregister** is the one-click rollback. It removes only the selected Agent's MCP entry named `s33d` from that Agent's user configuration. It does not uninstall or delete S33D, the bridge executable, a project, or model data. If configuration access is denied, S33D reports that permission is required instead of attempting to remove anything else.

## Deadlines, cancellation and safe retries (protocol v2)

Update the desktop host and bridge together. Raw TCP writes now require a top-level `idempotency_key` of 1–128 UTF-8 bytes; JSON `id` is only response correlation. MCP `s33d_execute` requires this key **before every write**. Read-only catalog/state/preview methods do not require one. CLI writes accept `--request-key`; when omitted, the CLI prints a generated key to stderr before submission and includes it in the response. Prefer an explicit key in automation.

- Commands have a 30-second queue/execution-response deadline. The UI checks it atomically before starting: an expired queued request returns `expired` and cannot later modify the model. `request.cancel` with params `{"idempotency_key":"your-key"}` cancels a queued write in the socket layer, even while the UI is blocked. Cancellation uses `expired` as its terminal status.
- Already started commands return `in_progress` when the wait ends; cancellation cannot interrupt native geometry or undo a started operation. Retry the **same method, params and key** to retrieve `queued`, `in_progress`, or the completed response. Same key with different method/params returns `idempotency_conflict`; it is never enqueued again inside the replay window.
- Replay records are retained for 10 minutes after completion, only for the lifetime of the same running listener. Restarting/disabling Agent Control or changing its port resets the ledger. After a reset or outside the window, inspect `document.get` before deciding what to do; replay is not durable exactly-once delivery.
- The ledger holds at most 256 writes and 8 MiB of serialized request identities/payloads. It refuses new writes with `server_busy` instead of evicting live/recent entries. Completed responses are cached up to 64 KiB each; larger replies retain a completion receipt, so query current state for details. An explicitly rejected, unqueued request may be retried with its original key. An expired/cancelled key remains terminal within the window; use a new key only for an intentionally new operation.
- Socket disconnection alone is not cancellation. Do not change keys merely because a client or MCP wait timed out. MCP keys must be supplied in advance so an outer tool timeout cannot lose the write identity.

Raw TCP request:

```json
{"id":1,"method":"primitive.create","params":{"kind":"box","width":40,"depth":20,"height":8},"idempotency_key":"session-unique-box-1"}
```

MCP tool arguments:

```json
{"method":"primitive.create","params":{"kind":"box","width":40,"depth":20,"height":8},"idempotency_key":"session-unique-box-1"}
```

CLI: `s33d-agent call primitive.create '{"kind":"box"}' --request-key session-unique-box-1`. Choose a fresh key for each intentionally different operation, retaining the old key for retries.

## MCP tools

- `s33d_workflow` returns the shipped text/photo/edit modeling guide.
- `s33d_reference` lists or reads optional project reference photos as native images.
- `s33d_render` returns a native model image and its document snapshot metadata.
- `s33d_describe` returns the current protocol version, all methods, parameter schemas, units, and examples.
- `s33d_state` returns the open document, selection, stable body IDs, feature information, sketch entities and regions, triangle counts, and world-space bounds.
- `s33d_execute` runs one method with one parameter object.

A reliable agent workflow is:

1. Read `s33d_workflow`, then call `s33d_describe` for the live catalog.
2. Read `document.summary` via `s33d_execute` before changing the model; use `s33d_state` or `body.get` when full detail is needed.
3. Execute one modeling command using returned stable IDs and a preselected idempotency key for each write.
4. Verify current state and, for appearance tasks, compare a fresh `s33d_render` image.
5. Correct or undo only this task's immediately preceding operation after checking that no unrelated user work intervened.

S33D rejects mutating Agent Control commands while an interactive Sketch, Draw 3D, Boolean, Mirror, Split, Section View, Extrude, or Revolve operation is open. Finish or cancel that tool in the desktop UI and retry. A pending workspace tool-exit confirmation additionally blocks desktop-dispatched requests except system.ping, system.describe and workflow.get, even if the tool uses Idle mode; transport-level request.cancel is unchanged.

## CLI examples

PowerShell accepts the request object as a single-quoted JSON argument:

```powershell
# Inspect the model
target\release\s33d-agent.exe call document.get

# Create a 40 × 20 × 8 mm box
target\release\s33d-agent.exe call primitive.create '{"kind":"box","name":"Base","position":[0,0,0],"width":40,"depth":20,"height":8}'

# Move body 1 by +25 mm along X
target\release\s33d-agent.exe call body.transform '{"body_ids":[1],"translation":[25,0,0]}'

# Save the complete project
target\release\s33d-agent.exe call project.save '{"path":"<ABSOLUTE_PROJECT_FILE_PATH>"}'
```

## Modeling methods

Call `system.describe` for the authoritative live catalog. Version 1 includes:

- Inspection: `system.ping`, `system.describe`, `document.get`, `body.get`
- Selection and objects: `selection.set`, `body.rename`, `body.set_visibility`, `body.delete`
- Creation: `primitive.create`, `sketch.create`
- Features: `feature.extrude`, `feature.revolve`
- Editing: `body.transform`, `boolean.apply`
- Files: `file.import`, `body.export`, `project.save`, `project.open`
- `file.import` accepts STEP, IGES, STL, OBJ, PLY, GLB/glTF and 3MF build geometry (units/components/Production references; printer settings are excluded; unsupported required extensions reject the import without partial objects). Native SolidWorks/Parasolid returns `conversion_required` with STEP/IGES export guidance. / `file.import` 支持 STEP、IGES、STL、OBJ、PLY、GLB/glTF、3MF build 几何（单位／组件／Production 引用；不导入打印设置，不支持的必需扩展会报错）；原生 SolidWorks/Parasolid 返回 `conversion_required` 及转换提示。
- `project.save` writes atomically and sets the active project path/save checkpoint. `document.get` reports `project_path` and `unsaved_changes`. `project.open` is an explicit replacement command (no interactive prompt); agents must save edits they need first. Pending user save confirmations block agent mutations. / `project.save` 原子写入并同步当前路径与保存基准；`document.get` 提供 `project_path` 和 `unsaved_changes`。`project.open` 明确替换当前项目，不弹交互确认；agent 应先保存需要保留的修改。用户保存确认未处理时，agent 不可修改文档。
- History and view: `history.undo`, `history.redo`, `view.fit`

### Sketch example

First create a profile:

```json
{
  "method": "sketch.create",
  "params": {
    "plane": "xy",
    "origin": [0, 0, 0],
    "name": "Plate profile",
    "entities": [
      {
        "type": "rectangle",
        "corner1": [-30, -20],
        "corner2": [30, 20]
      },
      {
        "type": "circle",
        "center": [0, 0],
        "radius": 8
      }
    ]
  }
}
```

Inspect the returned `sketch.regions` array, then extrude the intended region:

```json
{
  "method": "feature.extrude",
  "params": {
    "sketch_id": 1,
    "region_indices": [0],
    "depth": 6,
    "symmetric": false,
    "name": "Plate"
  }
}
```

`xy`, `yz`, and `zx` refer to world-coordinate datum planes. A custom plane uses `"plane":"custom"` with finite `origin` and non-zero `normal` arrays.

## Sketch Trim and view recenter (2026-09-19)

`sketch.trim.preview` and `sketch.trim.apply` expose the desktop Trim geometry
logic without mouse automation. They operate on a committed **Sketch** in Idle
mode, not on a solid's embedded source profile or an Equation Curve.

1. Read `body.get` for the sketch's `plane`, `entities` and current `regions`.
2. Call `sketch.trim.preview` with `{"sketch_id":1,"point":[10,0],"tolerance":0.1}`.
   The point is in that sketch's local **UV coordinates**, and tolerance is in
   **millimeters**, not pixels. Use the returned plane basis to map world positions.
3. Inspect `removed`, `replacement`, `entity_index`, `entity_count_after` and
   `regions_after`. This response is read-only; `dependents_validated:false` means
   the preview has not attempted to rebuild downstream solids.
4. Call `sketch.trim.apply` with the **same three parameters** plus
   `"preview_token":"<token returned by preview>"`. The token binds the sketch
   contents and plane, point and tolerance. A changed sketch or changed input
   returns `stale_preview`; obtain a new preview rather than retrying blindly.
5. Success returns the updated `body` and `rebuilt_body_ids`. Query fresh region
   indices before the next Extrude/Revolve. The sketch body ID is retained and
   the operation is one Undo/Redo step.

Trim removes the interval between endpoints/intersections nearest the requested
point. An isolated line or circle can be deleted entirely. Composite outlines
(rectangle/polygon/path/text) become independent remaining segments and lose
their whole-object parameterization. Open contours cannot form extrusion regions.
Supported sketch dependents are rebuilt before committing: `rebuild_failed`
leaves the document, selection and history unchanged. `no_hit` means there was
no trimmable segment within tolerance; `busy` means finish the desktop tool first.

`view.recenter` with `{}` uses visible document objects only, restores perspective
and centers/fits them in the viewport. It requires Idle mode and does not change
geometry or Undo history. Existing `view.fit` remains available.

The three MCP tool names remain unchanged; use `s33d_execute` for these methods.
Refresh `s33d_describe` after upgrading the desktop app. Keep `s33d-app` and
`s33d-agent` from the same build; these methods are not present in the older Alpha
Preview installer.

## Direct JSON transport

Tools that do not support MCP can connect to `127.0.0.1:53433` and send one request per line:

```json
{"id":"request-1","method":"document.get","params":{}}
```

S33D sends one response line with the same ID:

```json
{"id":"request-1","ok":true,"result":{"name":"Untitled Project","unit":"mm"}}
```

Errors are structured and do not close the desktop app:

```json
{"id":"request-2","ok":false,"error":{"code":"busy","message":"S33D is currently in Sketch mode..."}}
```

## Current boundary

Agent Control version 1 focuses on deterministic, ID-based construction and editing. Operations that depend on visual sub-entity picking—such as choosing a particular imported face or fillet edge—remain interactive in the desktop UI until S33D has general stable face/edge topology IDs.


### 2026-11-07: 3-view Drawing capability boundary / 三视图能力边界

The desktop **3-view Drawing** uses the shared Objects selection and keeps the 3D workspace until Generate succeeds. Users can edit and drag dimensions, drawing text, feature labels and title-block text, while 3D projection lines stay fixed. Verified inclined cylinder/bore rims retain true-diameter measurements. Choose **Export > SVG / DXF / PDF / EPS**. SVG/DXF/EPS write one file per sheet; EPS embeds Type 42 text and PDF contains all sheets with an embedded Chinese font. Text positions and replacements persist in the project. DXF coordinates are paper-space mm at the stated fitted scale, not 1:1 model coordinates, and dimensions are non-associative. Projection is tessellation-based, not exact HLR. `project.save` stores the current drawing snapshot and edits in `.s33d`; `project.open` restores them. Wait for projection generation before saving. Older projects without drawing data restore an empty drawing. Transient interaction state and drawing Undo/Redo history are not saved. Native DWG is not included.

桌面 **三视图工程图** 继承 Objects 选择，生成成功前保留三维工作区；可修改并拖动尺寸、图中文字、特征标号和标题栏文字，3D 投影线保持固定。可信斜向圆孔/圆柱端口使用真实直径。**导出 > SVG / DXF / PDF / EPS** 中，SVG/DXF/EPS 每页一个文件；EPS 内嵌 Type 42 字体文本，PDF 合并全部图纸并嵌入中文字库。文字位置及替换随工程保存。DXF 为按比例缩放的纸面毫米坐标，不是 1:1 模型空间，尺寸不关联几何约束。投影基于网格，不是精确 HLR；`project.save` 保存图纸快照与编辑，`project.open` 恢复；生成期间须等待后保存。旧工程无图纸字段时恢复为空，临时交互与标注撤销历史不保存。暂不内置 DWG。

`system.describe.desktop_drawing` reports this desktop capability with `agent_commands: false`. No `drawing.*` MCP/CLI methods currently exist; `body.export` remains a 3D geometry export and must not be given SVG/DXF/PDF/EPS. Do not infer commands from the desktop menu. / `system.describe.desktop_drawing` 明确返回 `agent_commands: false`；目前没有 `drawing.*` MCP/CLI 命令，`body.export` 仍只导出三维几何，不能把 SVG/DXF/PDF/EPS 传给它，也不能根据桌面菜单猜测命令。


### 2026-11-08: Background 3D exports / 后台三维导出

`body.export` supports STEP/STP, STL, OBJ, PLY and GLB (the `gltf` alias writes the existing binary GLB format). Desktop-dispatched exports run in the background and show progress. A success response means the complete file has been published. Failure, desktop Cancel/Esc, project reset, or a stale result leaves the final destination unchanged. The model and Undo history are not modified. Closed mesh STEP exports condition numerical slivers and validate topology, self-intersections, volume and sampled boundary distance; substantial defects return `export_error`. Exact surfaces retain B-rep geometry. No export-preference or extra cancellation method was added.

`body.export` 支持上述三维格式；桌面派发后后台运行并显示进度，文件完整发布后才返回成功。失败、桌面取消/Esc、工程重置或结果过期均不改写最终目标，不修改模型或撤销记录。STEP 对网格细屑进行受限清理，并验证闭合拓扑、自交、体积与边界采样偏差；明显缺陷返回 `export_error`。精确曲面继续保留 B-rep。传输层 `request.cancel` 仍只取消尚未执行的排队请求；运行中返回 `in_progress`，请用相同幂等键和参数查询结果。桌面取消导出后原请求返回 `cancelled`。后台计算期间，除 system.ping/system.describe/workflow.get 外的桌面请求返回 busy。

### 2026-11-20: STEP export and desktop Pocket / STEP 导出与桌面切除

`body.export` uses the same STEP solid validation as desktop export. For legacy meshes, the converter can remove a folded seam when its opposing surfaces coincide within the welding tolerance. The output still needs to pass bounds, volume, topology and boundary checks. A local regression exported and reimported a repaired mesh as one closed solid.

`body.export` 与桌面导出共用 STEP 实体验证。旧网格的折叠接缝仅在其反向表面于焊接容差内重合时可清理。输出仍须通过包围盒、体积、拓扑和边界检查。本地回归已将修复后的网格导出并回读为一个闭合实体。

In the desktop UI, press **P** on a selected closed region. The app uses a selected target, an attached or linked visible body, or a unique nearby candidate. Agent modeling can create a cutting solid with `feature.extrude`, then call `boolean.apply` with `operation: "subtract"` and the target and cutter body IDs in that order.

在桌面上选好封闭区域后按 **P**，应用会查找已选目标、附着或关联的可见实体，或唯一邻近候选。通过 Agent 建模时，可用 `feature.extrude` 创建切除体，再调用 `boolean.apply`，将 `operation` 设为 `"subtract"`，并按目标体和切除体的顺序传入 ID。

### 2026-11-22: Desktop surface menu and export defaults / 桌面表面菜单与默认导出

Right-click a selected bounded planar face or closed sketch region to open its desktop modeling actions. A supported cylindrical wall offers Generate thread. The desktop save dialog suggests the project name and STEP (`.step`). For MCP `body.export`, provide the target `path` with a recognized suffix or set `format` explicitly.

桌面选中有封闭边界的平面或草图区域后，可右键打开相应的建模操作。受支持的圆柱壁可打开螺纹生成。桌面保存对话框建议使用项目名及 STEP（`.step`）。使用 MCP 的 `body.export` 时，`path` 应包含可识别的后缀，也可显式指定 `format`。

## Checked sketch regions (2026-11-09)

Region indices belong to one profile revision. After editing geometry or the plane, query the current regions and supply fresh indices; desktop region selections are cleared on owner/revision changes. Extrude and Revolve validate the complete region before allocating a result. Invalid caps fail with a geometry error, never a partial success. Text, gear and thread caps use the same checked triangulation. No new MCP method is introduced.

Open branches and graph bridges between closed loops are excluded from derived material boundaries; original sketch entities remain unchanged. Genuine dividing cycles are retained. The desktop Properties panel keeps validation errors for the current owner/revision/region after a brief notification expires. Region numbering can change after boundary extraction fixes: query the loaded profile again rather than reusing previously observed indices.


## Photo/model workflow and native images (2026-11-16)

The connection menu is now Settings > Agent Connect. The visual Agent interprets photos in its own chat, calls S33D to model and checks native model images. Importing photos into S33D is optional project storage or alternate delivery, not a prerequisite. See [PhotoToModel.md](PhotoToModel.md) for the concrete connection and modeling steps. The built bridge supplies the same workflow as the shipped skill; a separate skill installation is optional.

- `s33d_workflow` gets the versioned text/photo/edit guide without a running desktop or developer directory.
- `workflow.get` via `s33d_execute` is the read-only compatibility route for older three-tool hosts connected to the updated desktop. Its guide matches `s33d_workflow`, including source hashes and the Agent-maintained execution-state template. All modes share `modeling-strategy.md` (2026-11-17.2); no automatic likeness judge or persisted plan is implied.
- `s33d_reference` lists imported photos or returns one registered photo as a native MCP image.
- `s33d_render` returns a native model image and its immutable snapshot metadata without changing the GUI view or model.
- `document.summary` via execute avoids repeated full feature/B-Rep responses. Legacy empty `s33d_state` remains unchanged.
- `reference.import/list/get/remove`, `view.get/set/render`, and `body.material.set` are in the live catalog. Writes retain idempotency keys and can additionally use `_expected` document/revision guards. A rendering worker never installs its snapshot back over the current GUI camera or selection.

The bridge negotiates only its supported MCP versions and advertises implemented resources, not prompts. Protocol errors and desktop errors preserve the old error semantics. Actual host image-display acceptance must be checked separately from CLI registration and loopback connectivity.


## Desktop guides and recent projects (2026-11-19)

The top-right workspace switch controls grid and RGB reference axes together, synchronized with View; Settings > Re-enable all hidden tips restores them. It is a display control and leaves snapping, geometry and the orientation gizmo intact. File > Open Recent keeps ten successful open/save paths in local preferences and uses the normal unsaved-changes guard and background loading. These controls are desktop-only; use published project.open with an actual path rather than inventing recent.* or guide-toggle commands. Native model images continue to exclude viewport guides.
