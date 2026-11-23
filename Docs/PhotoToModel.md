# Photo to model with your existing Agent

Your visual Agent reads and interprets the photos in its own chat, then calls S33D to construct and revise the model. S33D provides geometry operations and native model images through its local MCP bridge; the Agent compares those images with the photos and corrects the model. Importing photos into S33D is optional reference storage or an alternate way to deliver images to the Agent, not a prerequisite for photo modeling. You do not need another AI account, a manually installed skill, Python, desktop-control software or a development checkout.

## Connect once

Open **Settings > Agent Connect**, enable the local connection, and use **Register / Update** for your existing Codex or Claude Code installation. This changes only its S33D MCP entry. S33D does not register automatically during installation. Use **Test connection** to check the desktop listener. Your Agent must support image understanding and MCP image results; reconnect its chat only when that host requires it to discover a newly registered server. Registration/listener success alone does not prove the host displays images.

In an Agent chat, provide photos of the same object and describe the desired model. Include a known dimension if available. For approximate appearance without a real dimension, the Agent can state a relative scale and continue. Engineering fit/tolerance needs real dimensions.

## User steps: photos in the Agent, modeling in S33D

1. Open S33D on the same computer as your Agent and leave it running.
2. Open **Settings > Agent Connect**, enable **Enable local control**, and select **Test Connection**.
3. Under **Codex**, select **Register / Update**. Reopen Codex so it discovers the updated S33D connection. This is a one-time connection step, repeated if the S33D installation moves or the registration changes.
4. In a local Codex chat, attach one or more photos of the same object and state the modeling requirements, a known dimension if available, and the destination for the editable `.s33d` project.
5. The Agent interprets the chat photos, uses S33D modeling operations, requests native model images, compares and corrects the result, and saves the editable project. Continue changes in the same chat with the existing model.

You can use **Copy modeling prompt** in Agent Connect even when no photos have been imported into S33D. Attach the photos in the Agent chat and add your request. If the Agent can already see the chat images, no local photo path, `reference.import`, registered asset ID or second upload is required. If it cannot see any image, provide an attachment or real path that the Agent can actually read; the optional S33D reference library below is another delivery route.

### 中文操作示例（Windows / Codex）

1. 打开 S33D，保持运行。
2. 在 **Settings > Agent Connect（设置 > Agent 连接）** 勾选 **Enable local control（启用本机控制）**，点击 **Test Connection（测试连接）**。
3. 在 **Codex** 一栏点击 **Register / Update（注册 / 更新）**，重新打开 Codex。首次连接或安装位置变化后执行此步。
4. 在本机 Codex 聊天中直接上传同一物体的一张或多张照片，说明尺寸、建模要求和工程保存位置。
5. Agent 看照片并分析结构，再调用 S33D 建模；Agent 读取 S33D 原生模型图，与照片对照、修改，最后保存可继续编辑的 `.s33d` 工程。

可直接发送：

> 请看我上传的这些照片，在已连接的 S33D 中建立同一物体的可编辑模型。已知整体高度为 120 mm。先核对外轮廓、比例和主要结构，再补细节；请读取 S33D 原生模型图与照片对照并修正。保存到 D:/Models/example.s33d。没有证据的隐藏结构请说明估算。

把示例中的尺寸和保存路径换成实际要求；没有真实尺寸、只要求近似外观时，可以声明相对比例。照片由 Agent 理解，S33D 执行建模并返回模型图。无需先在 S33D 导入照片，也无需手动安装 skill、Python 或开发环境。

## Optional: store reference photos in the S33D project

Use **Import reference photos…** in **Settings > Agent Connect** if you want photos saved with the project, or want the Agent to read them through `s33d_reference`. The Agent can alternatively call `reference.import` with an actual available local PNG/JPEG path. A chat attachment does not automatically become a path readable by the S33D server; that has no effect on the Agent's ability to use images it already sees in chat. Do not hunt temporary attachment paths.

The following limits apply only to this optional S33D import: PNG and JPEG are supported, EXIF rotation is applied, and images are reduced to at most 2048 pixels per side with metadata removed. Limits are 24 MiB / 32 megapixels per source and 16 photos / 32 MiB encoded per project. They are not the Agent host's attachment limits.

Reference images are embedded in the `.s33d` project when you save. The original files are no longer required to reopen that project. Imported photos are normalized references, not archival originals. Keep your original photography separately if you need its full resolution. Import/remove marks the project unsaved and is separate from geometry Undo/Redo. Projects modeled from chat photos alone contain no reference images unless you explicitly import them.

## Model, compare and continue

The Agent obtains the shipped workflow, distinguishes observed shape from estimates and unknowns, and keeps common dimensions and boundaries for connected regions. It checks overall shape, structure and identity features before details. A major mismatch should trigger a correction or a different construction, rather than more detail. These checks are internal; an already authorized request does not require approval at every stage.

Each native model comparison should lead to a concrete decision and a recheck of affected views. Appearance, geometry type/validity, dimensions/function, editability and save/reopen are separate acceptance checks. A later natural-language request can change the same objects while reusing photos, dimensions and accepted decisions. Specify a save location once; the Agent should confirm successful saving.

Native images use S33D geometry, colors and fixed lighting. They exclude the grid, selection accents, active drafts and section-tool clipping. They do not change the desktop camera or model. A working native wgpu rendering adapter is required; if unavailable, image output reports an error. Rendering includes the document snapshot identity, version, camera and object IDs so an earlier image cannot be mistaken for the latest edit.

Appearance reconstruction may use separate bodies and open surfaces. Similar appearance does not certify a closed printable solid or manufacturing dimensions. Cross-restart task automation, arbitrary surface repair and photogrammetry are outside this version.

## For integration authors

The original `s33d_describe`, empty `s33d_state`, and `s33d_execute` remain compatible. Added tools are `s33d_workflow`, `s33d_reference`, and `s33d_render`. The workflow and skill are compiled from one repository source. Guide version 2026-11-17.2 returns an aggregate SHA-256, a per-file source manifest and a compact execution-state template. Every route includes the common modeling strategy. The executing Agent maintains the filled state in its conversation/checkpoint; S33D does not persist a workflow plan or automatically judge likeness. Hashes prove which guide was supplied, not that an Agent followed it.

For a host still exposing only the original three tools, use `s33d_execute` with `method: "workflow.get"` and `params: {"mode":"auto"}` (also photo/model/edit/geometry). This read-only alias requires the updated desktop and returns the same guide as `s33d_workflow`. The guide also appears at `s33d://workflow/modeling` as an MCP resource. Prompts are not advertised.

Use `document.summary` for compact checks; `document.get` / `body.get` retain full-detail behavior. New methods are `reference.import/list/get/remove`, `view.get/set/render` and `body.material.set`; inspect the live catalog for exact parameters and limits. Material writes currently support RGB only, in the same 0..1 convention as the native viewport; other material fields are preserved.

Writes can include `_expected: {"document_id":"…","revision":123}` in their parameter object. The desktop rejects a stale token before executing. Opening/replacing a document changes its session identity; Undo/Redo advances its revision. Camera changes have an independent view token. Same-key, same-payload completed retries replay before these checks, within the existing listener lifetime and ten-minute retention. These tokens are not persistent cross-restart exactly-once receipts.

The stdio bridge supports MCP 2024-11-05, 2025-03-26, 2025-06-18 and 2025-11-25. It returns a supported version when a requested version is unknown; the client disconnects if it cannot use that version. Images use MCP image content blocks, with snapshot metadata in a separate text block. Geometry requests retain their 1 MiB limit; images are bounded responses, not arbitrary file-reading commands or base64 image uploads in geometry requests. See the [MCP lifecycle specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle) and [tool content specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
