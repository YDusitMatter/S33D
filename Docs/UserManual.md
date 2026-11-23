# S33D 操作手册 / Operation Manual

2026-11-11：启动页与工作区按当前 Windows 显示器工作区、DPI 和边框适配；小窗口中的首次指引、齿轮、纹理、轮胎及轮毂参数可滚动，保留底部操作。相机栏宽度稳定，纹理预览限于预览框；螺纹保存预设改为独立窗口。详见中英文手册“小窗口与显示缩放 / Small windows and display scaling”。自动布局验证不替代物理跨屏与高 DPI 验收。

2026-11-10：新增主显示器居中的无边框工程图启动页，至少显示 2 秒，启动文件加载较慢时延长显示；后台工程/网格加载及读取进度；完整精确圆柱面可直接改直径并一步撤销。螺纹生成参数双列并随可见空间展开，warning、修正与生成按钮保留在下方；“按公称直径修正”保留指定 nominal diameter，实际调整孔径 / 毛坯，使其符合指定螺纹参数。经验证的 Sketch Extrude/Pocket 内孔也支持修正；分段实体转为精确实体后改径，共用源草图保持不变，独立导入网格仍只读。部分加工后，未加工完整柱面段可独立重选，后续偏移从选中段起点计算。边界和用法见中英文手册的“启动、后台加载与圆柱直径编辑 / Startup, background loading and cylinder diameter editing”。

草图拉伸内孔的“选孔壁 → 设定参数 → Correct → Generate”完整流程及打印建议的先后顺序见 [螺纹打印与修正说明](ThreadPrinting.md)。

2026-11-08：打印建议旁的“...”打开独立设置窗口，关闭后再点 suggest 更新打印齿形与余量；左栏新增 Group/Ungroup。倒角/圆角 Apply 即时生效，同位置更新、换位置累积；Exit 或双击/三击可保留已应用结果并返回选择。已修复双孔草图选区缺块与石墨主题 Extrude Done 按钮显示。

本手册覆盖 S33D 当前主要操作。可直接在 Help 下拉菜单顶部按中文、英文工具名或快捷键搜索，并在紧凑的主题菜单中查看说明。

新增连击退出确认：显示当前工具名，确定保留当前状态并退出，取消继续编辑；可勾选“不再显示”。Settings 中“重新启用所有隐藏提示”可恢复此提示及首次使用提示。

三维工作区的无冲突工具现统一支持双击/三击先将当前有效状态 Applied and Done、收起菜单，再选面/整个对象；普通退出也提交最新参数，已应用操作不会重复；无效输入保留编辑并提示。逐点绘制与基础体放置阶段保留构造流程。Draw 3D 工具内单击实体即可选择并编辑，Getting Started 已补充中英文说明。详情见[中文手册](UserManual.zh-CN.md) / [English guide](UserManual.en.md)。

2026-11-07：右栏螺纹入口仅在选中可识别柱面时显示，点击后先看几何信息，再选择匹配预设或 Custom；修改参数自动转为 Custom。新增当前直径的打印建议（默认 FDM 0.4 mm 喷嘴、0.2 mm 层高，可改喷嘴/层高或选树脂）、成对径向余量、红色 warning 及个人预设持久保存。详见[中文手册](UserManual.zh-CN.md#螺纹生成) / [English guide](UserManual.en.md#thread-generator)。

2026-11-05：三视图工程图及标注、文字和标题栏随 `.s33d` 保存，打开工程时恢复；旧工程无图纸时为空。仅修改图纸也有未保存提示，投影生成期间请等待后保存。Drawing snapshots and edits now persist in `.s33d`; projection must finish before saving. 详见中英文手册第 24 节。

2026-11-05：STL 内孔和外圆柱可在通过闭合端环与完整侧壁检查后双击选取整圈；台阶孔分段选择。Mesh cylinder/bore selection now certifies complete bands and selects stepped bores separately. 拟合误差边界与螺纹用法见中英文手册。

2026-10-31：新增编组/取消编组、中心轴线对齐、螺钉/螺帽生成器；启用插件后从菜单栏下方工具栏展开/收起。首次指引可选择并保存外观，原版改名为“简洁”。用法见[中文手册](UserManual.zh-CN.md#编组中心轴线对齐与紧固件插件) / [English guide](UserManual.en.md#groups-center-axis-alignment-and-fastener-plug-ins)，尺寸来源见[紧固件生成说明](FastenerGenerators.md)。

插件菜单名称/开关/打开操作左对齐，开关右侧启用、左侧停用；暗色初始指引使用清晰的正文/说明文字色。MCP 可从 `document.get.groups` 读取编组，桌面新功能与自动化边界见 [Agent Control](AgentControl.md)。Plug-ins use left-aligned names and left/right enable switches; dark Getting Started uses theme text colors. MCP exposes read-only logical groups; generator and axis-alignment workflows remain desktop tools.

2026-10-30：Sketch / Draw 3D 可展开收起；新增 Close Project、Save Project As、未保存修改确认和 IGES 导入。项目与 SolidWorks 文件兼容说明见[中文手册](UserManual.zh-CN.md#4-工程与文件) / [English guide](UserManual.en.md#4-projects-and-files)。

2026-10-29：新增实体螺纹生成，支持公制、UNC/UNF、BSPP/BSPT、打印预设及批量/连续应用。操作、底孔尺寸和采样网格限制详见[中文手册](UserManual.zh-CN.md#螺纹生成) / [English guide](UserManual.en.md#thread-generator)。MCP 方法见 [Agent Control](AgentControl.md#solid-threads-2026-10-29)。

2026-11-08 收尾后补充：孔口处仍可见的随体圆形草图与该孔端环吻合时，不会阻挡内孔壁的悬停及双击选择；无需先隐藏草图。编辑该草图时，在 Objects 列表选择它并使用 Edit Sketch。其它草图区域仍按原规则选择。

完整圆柱/孔壁可双击后右键生成螺纹，支持解析及可识别的样条/分片壁面。Objects 多选右键提供捕获目标组的批量可见、颜色与删除，一步撤销；详细支持边界见双语手册。

螺纹工具现于请求两端各延伸一圈后裁切到起始偏移/长度范围；触及实体端面时沟槽贯通，局部螺纹仍只加工选定区间，不自动添加入口倒角或生产退刀槽。V 形外牙/内牙预设默认牙底平段比例分别为 1/6、1/8；125 圈限制按请求长度/螺距计算，内部额外两圈不占额度。Thread cuts reach physical ends while preserving explicit partial-thread limits; V-profile external/internal preset root-flat defaults are 1/6 and 1/8, and the requested length/pitch limit remains 125 turns.

2026-09-06：草图新增手绘与可编辑文字轮廓；Draw 3D 创建预览支持反复拖动移动/关键点调尺寸；插件菜单逐项显示启用开关。详细中英文用法见 [中文手册](UserManual.zh-CN.md#手绘与文字) / [English guide](UserManual.en.md#freehand-and-text)。

## 1. 界面布局 / Interface

2026-09-19：Sketch 新增 Trim（修剪），悬停橙红色预览、单击删除顶点/交点间片段，支持逐次撤销重做。用法见 [中文手册](UserManual.zh-CN.md#修剪trim) / [English guide](UserManual.en.md#trim)。

- 左侧是建模工具栏：选择 Sketch、Draw 3D、Extrude、Boolean、Mirror 等命令。
- 中央是 3D 工作区：查看模型、选择 edge / surface / object，并操作 Gizmo。
- 右侧是 View、Properties、Beginner Guide 和 History。
- Objects 浮动窗用于查看、选择、隐藏、改名、改色和删除对象。
- 底部状态栏显示提示、当前 Unit、Step 和 Snap。

## 2. 选择对象 / Selection

- 工作区单击选择边（Edge）。
- 工作区双击选择面（Surface）。
- 选中封闭平面或草图区域后，右键可新建草图、拉伸（E）、切除（P）、旋转或取消选面（Esc）。受支持的圆柱壁提供生成螺纹。
- 工作区三击选择整个物体（Object）。
- 按住 Ctrl 或 Shift 可追加或取消多选。
- Ctrl+A 选择全部可见实体；当当前工具不支持多选时，程序会清空冲突选择并提示。
- Objects 窗普通单击选择对象并确定区间起点，Ctrl 单击追加/取消，Shift 单击选择起点到当前项之间的完整区间，Ctrl+Shift 追加区间。隐藏项也可选择，不自动显示；多选后右键“可见”可统一显示。

## 3. 视图与相机 / View and Camera

- 鼠标中键拖动 Orbit，右键拖动 Pan，滚轮 Zoom。
- View 中的 Iso、Top、Front、Right 切换标准视角。
- Perspective / Orthographic 切换透视和正交投影。
- Fit 使全部可见模型适合当前窗口。
- 数字键 0、1、3、7 对应 Iso、Front、Right、Top；数字键 5 切换投影。

## 4. 工程与文件 / Project and Files

- File → New Project 新建工程。
- Save Project 将对象、几何、History、Undo/Redo、Selection、Camera、Grid 和 Snap 保存为 .s33d。
- Open Project 打开 .s33d 工程。
- Import 支持 STEP、IGES、STL、OBJ、PLY、GLB/glTF、3MF；导入完成后新对象默认选中。
- File → Export… 在系统保存对话框选择 GLB、OBJ、STL、PLY、STEP 或已生成工程图的 SVG/DXF/PDF/EPS。读入与输出目录分别记忆。
- 剖面不相交时保留完整实体，相交后正常剖切；拖动箭头沿法线移动，方程同步。
- 数值工具激活后首次聚焦第一个可用参数；Edit 增加 Copy/Cut/Paste。默认外观为 Graphite Studio，保留已有用户选择。
- 3MF 几何支持与边界详见 [中文指南](UserManual.zh-CN.md) / [English guide](UserManual.en.md)。

模型导出默认用当前项目名，格式为 STEP（`.step`）。

## 5. Objects 对象管理

- Visible 复选框控制对象显示；隐藏对象不会参与选择和布尔运算。
- 右键对象可 Rename、选择 Color、切换 Visible 或 Delete。
- 修改颜色时在颜色窗口中选择颜色，再按 Apply Color；Cancel 放弃修改。
- 双击可编辑的 Sketch、Draw 3D 或特征对象，可重新进入相应参数界面。

## 6. Sketch 草图

- 单击 Sketch，选择 XY、YZ、ZX、模型平面或 Linear equation 平面。
- Line（L）画直线，Arc（A）画圆弧，Circle（C）画圆，Rect（R）画矩形，Poly（G）画多边形。
- Pick（V）选择草图图元；Ctrl/Shift 支持多选，右侧显示各图元 Properties。
- Key points 显示端点、中点和中心；Magnetic snap 吸附草图及参考物体的关键点。
- Done 完成当前绘图工具；Exit Sketch 保存并退出草图。
- Esc 优先结束当前链，再次 Esc 退出并保留已完成的图元。
- 可在属性中为直线、圆弧、矩形和多边形选择锁定点。默认锁定首个放置的点，多边形默认为中心。数值改形和旋转固定该点，移动时该点随图元一起移动。

## 7. Sketch Move、Rotate 与 Align

- Sketch Move 支持多选、数值 Offset、Step 和四方向即时移动。
- Sketch Rotate 正角度为顺时针；可即时顺/逆时针旋转 90°。
- Custom Pivot 使用草图平面上的世界坐标指定旋转中心。
- Sketch Align 选择两个或更多图元，锁定参考图元，选择世界 Axis 与 Min/Center/Max 后 Apply。
- Move/Rotate 的虚线是待提交预览；Apply 提交，Reset 恢复预览值。

## 8. Draw 3D 基础体

- Sketch/Draw 3D 创建中可切换其它顶层工具，有效改动先 Apply/Done/Exit；空创建不产生对象，失败保留现场。Switching creation tools commits valid work first; empty sessions add nothing and failed commits preserve the session.

- Draw 3D 可创建 Box、Cylinder、Sphere、Cone 和 Torus。
- 球体现在单击放球心，右栏显示球心坐标，改半径不移动球心；轻量预览后 Apply 验证精确实体，再 Done。Sphere placement/Properties use the center; resizing keeps it fixed. Apply validates exact geometry before Done. Saved/Agent position retains the legacy base convention for compatibility.
- 在右侧设置 Base Position、Width、Depth、Height、Radius 等参数。
- Done 创建当前基础体；仍在 Draw 3D 状态时可重新选中并编辑参数。
- Apply 后按 Esc 会提交当前基础体并退出 Draw 3D。
- Exit Draw 3D 结束基础体绘制。

## 9. Extrude、Pocket 与 Revolve

- 选择闭合 Sketch region 或有固定边界的 Surface 后可直接 Extrude、Pocket、Revolve。
- Extrude 设置 Depth，可用 Symmetric 对称拉伸。
- Pocket 采用已选实体、唯一关联的可见实体，或根据轮廓位置推断唯一目标。若有多个候选，在工作区单击目标。设置 Depth 或勾选 Through 完全贯穿目标实体。
- Revolve 先选择旋转轴，再设置顺时针为正的 Angle。
- 右侧 Done/Apply 提交；Esc 取消当前操作。

## 10. Move、Rotate 与 Scale

- Move 支持一个或多个对象；拖动 Gizmo 或输入世界 X/Y/Z 偏移。
- X+/X-/Y+/Y-/Z+/Z- 按当前 Step 立即移动。
- Move To 将所选对象组的共同中心移动到目标坐标。
- Rotate 支持世界轴、模型直边或草图直线作为轴；+90°/-90° 按钮立即旋转。
- Scale by percent 按百分比缩放；Scale to size 缩放到指定包围盒尺寸。
- Uniform Scale 保持比例；关闭后允许各轴独立缩放。

## 11. Align 对齐

- Align 可在零选择或单选择时先进入，再从 Objects 或工作区加入对象。
- 第一个有效实体默认作为锁定参考，也可在对象列表中更改 Lock。
- 选择 Axis 与 Min、Center 或 Max，预览确认后 Apply。
- 对象较多时列表可滚动，Axis、Mode 和 Apply 保持优先显示。

## 12. Boolean 布尔运算

- Union 支持多个对象；选中对象统一改变预览色，Apply 后合并。
- Subtract 计算 Original Object (A) − Subtracting Object (B)，Swap objects 交换两栏，最后 Apply 才执行。
- 点击任一栏展开可滚动的可见实体名称/编号菜单，也可在工作区或 Objects 点选。选到另一栏已有对象时，另一栏自动清空等待重选；两栏齐备后才允许 Apply。
- Intersect 同样选择 Original object 与 Tool object，保留相交部分。
- 单击操作数输入框可重新选择；Keep originals 决定是否保留原对象。
- 布尔运算必须按 Apply 才正式生效。

## 13. Mirror、Split 与 Section View

- Mirror：先选对象，再选镜像面，最后 Apply。
- Split：选择 Object 和 Cutting plane，Apply 后生成独立实体。
- Section View 只改变观察剖面，不修改几何；Esc 退出。
- 平面可选 XY/YZ/ZX、模型平面或 ax+by+cz=d Linear equation。
- Flip side 显示平面的另一侧；Keep original 控制是否保留源对象。

## 14. Chamfer 与 Fillet

- 进入 Chamfer/Fillet 后选择一条边或连续边链。
- Chamfer 使用 Distance，Fillet 使用 Radius。
- 凸斜边 Radius 是两面相切圆的半径，不是沿面退让距离；内角越尖需要的空间越大。已保存的结果不会因软件更新自动变化，应重新编辑特征并 Apply 重建。
- Apply 生成当前预览；Done 提交并允许继续选择下一对象。
- Objects 中可重新选择特征并修改参数；删除中间特征会重建后续依赖。

## 15. Array、Copy、Paste 与 Delete

- Array 可同时启用 X/Y/Z 轴，分别设置方向、Spacing 和 Copies。
- Copies 数字包含原对象。
- Copy 复制所选对象，Paste 按递增偏移生成副本。
- Delete 或键盘 Del 删除当前所选对象。

## 16. Units、Step 与 Snap

- Settings → Units 可选择 Millimeter、Centimeter、Meter、Inch 或 Foot。
- 模型内部和工程文件始终保存为毫米；切换单位只换算界面输入与显示，不改变模型真实尺寸。
- 底部 Unit 显示当前单位；Step 自动换算为该单位下的等效数值。
- Snap 开启时，草图和移动按 Step 对齐网格；关闭后允许连续坐标。

## 17. Undo、Redo 与 History

- Ctrl+Z 或 Edit → Undo 撤销。
- Ctrl+R 或 Ctrl+Shift+Z 或 Edit → Redo 重做。
- Undo/Redo 同时恢复对象选择和文档几何；Sketch 编辑使用草图会话内时间线。
- History 显示最近操作，但选择记录不一定都对应可撤销几何事务。

## 18. Calculator 科学计算器

- 启动时扫描程序旁的 `Plug-ins/*/plugin.json`，开发构建使用项目根目录；只接受受支持的 builtin 插件，修改清单后需重启。Startup discovers supported built-in manifests from the executable's sibling Plug-ins folder (workspace root for Cargo builds); restart after manifest changes. Arbitrary DLL/script loading is not supported.

- 在 Plug-ins 中启用或直接打开 Calculator。
- 支持 +、−、×、÷、幂、括号、Ans、π、e、sqrt、sin、cos、tan、asin、acos、atan、ln、log、abs 和 exp。
- DEG / RAD 切换三角函数角度制与弧度制。
- Enter 或 = 计算；C 清空；⌫ 删除末尾字符。

### Generators / 生成器与表面插件

- Plug-ins 可启用 Gear Generator、Tire Generator、Wheel Rim Generator 与 Surface Texture。
- Tire Generator 按外径、轮辋直径、断面宽、胎纹深度和重复数生成 Highway Rib、Block、Chevron 或 All-Terrain 胎纹。
- Wheel Rim Generator 生成 Five Spoke、Six Spoke、Split Five Spoke 或 Multi-Spoke 轮毂，可编辑轮心、中心孔和螺栓孔参数。
- Surface Texture 需先选择平面或参数化圆柱侧面，可生成 Straight、左右 Diagonal 或 Diamond 滚花，并设置 Pitch、Groove Width、Depth、Angle 与 Margin。
- 生成对象保留参数；选中后重开插件即可保持 Object ID 更新。Surface Texture 会隐藏并保留源对象，形成非破坏依赖。

## 19. 设置与新手模式 / Settings and Beginner Mode

- Settings 可切换 Language、Font、Font Size、Appearance、Accent Color、Units、Getting Started 和 Beginner Mode。
- Getting Started 窗口可直接切换中文/English，并启用新手模式。
- 右侧 Beginner Guide 的关闭键会关闭并保存新手模式状态。

## 19.1. 外观与工具图标

在**设置 > 外观**中选择：**原版**（默认，保留紧凑的纯文字工具栏和原有蓝／青／橙强调色）、**A · 精密经典**（暖白面板、深灰工作区、青色控件）、**B · 石墨工作室**（深色面板与工作区、暖橙色控件）或**C · 瓷白工坊**（浅色面板与工作区、靛蓝色控件）。

三种新样式共用原创 CAD 矢量图标与文字标签，工具顺序不变；工具栏略宽，必要时可滚动，草图与绘制三维工具组可以折叠。预设各有强调色，“强调色”子菜单仅作用于原版。选择立即生效并保存，重启后保留；只改变界面与工作区配色，不修改几何、对象颜色、选择状态或相机。

## 20. 常用快捷键 / Shortcuts

- Ctrl+Z：Undo；Ctrl+R：Redo；Ctrl+A：Select All。
- Del / Backspace：Delete；Esc：Cancel 或退出当前工具。
- S 打开 Sketch，E 打开 Extrude，P 打开 Pocket，O 打开 Revolve。Q 也可打开 Pocket。
- L：Line；A：Arc；C：Circle；R：Rectangle；G：Polygon；V：Pick。
- M：Move；T：Rotate；F：Fit View。

## 21. 常见问题 / Troubleshooting

### 2026-09-08：空间曲线与精确几何 / Spatial curves and exact geometry

09/10 更新：可见共面方程曲线共同划分选区，可双击圆环或两条开放线实际围成的区域再拉伸。多曲线结果保存完整轮廓快照，后续不随单一曲线重建；单曲线关联行为不变。精确实体平面口沿直接拉伸使用原始曲边/内孔，避免球壳切口预览与提交边界不一致；对称平面拉伸按总深度计算。

09/10 update: visible coplanar equation curves share a region arrangement. Select a ring or genuinely closed region before extrusion. Multi-curve features freeze the full profile; single-curve links remain live. Exact planar-face extrusion preserves original boundary wires, including shell rims; symmetric depth is the total depth.

Draw Equation 已独立与 Sketch / Draw 3D 并列，提供 x(t)、y(t)、z(t) 或 R(t)、theta(t)、phi(t)，手动定义域与单位。输入实时预览，Apply preview 后 Done / Exit 提交，转选其它工具先提交有效内容。Curve 可回选改方程、保存重开及撤销重做；同平面的闭合区可像 Sketch 一样双击高亮并 Extrude/Pocket。非平面曲线不会压平为面域。

Draw Equation is a top-level tool with live input preview, Apply preview and Done / Exit. Coplanar enclosed regions support Sketch-like selection and Extrude/Pocket; spatial curves are not flattened. Re-edit from Objects with stable IDs; definitions and linked planar extrusions support persistence and Undo/Redo. Switching tools commits valid work; invalid edits preserve the editor.

Draw Equation 的 Surface 模式接受 u/v 参数、XYZ 或球坐标、两组定义域与 4–64 细分；后台 B 样条插值生成曲面。Fill 仅填充已经闭合且有效的曲面，不自动封口。选择未填充曲面后 Extrude 沿局部法线加厚，默认对称且输入为总厚度；预览成功后完成。可回选编辑方程、保存重开及撤销重做。加厚结果冻结源快照，不跟随方程后续变化。

Surface mode uses u/v domains and smooth B-spline interpolation, not a symbolic exact surface. Fill never caps open sheets. Equation-surface Extrude follows local normals and defaults to symmetric total thickness. Agent methods are surface.equation.preview/apply and feature.offset jobs with SymmetricThickenSurface; consult the live catalog and language-specific manuals.

Offset / Shell 需要保留 B-Rep 的 STEP 对象，分别支持曲面偏移、选移除面的实体抽壳、开放曲面加厚。预览计算完成后 Done 才提交；失败保留原对象。旧网格不能恢复真实曲面；必要时重导 STEP。结果保存冻结源快照，工程版本 2 保留精确几何。

Exact offset/shell requires original STEP surfaces. Choose offset, hollow with removal faces, or open-surface thickening. Done commits a ready preview; failures preserve the original. Existing mesh-only files cannot recover lost smooth surfaces. Outputs retain frozen source snapshots. See the language-specific manuals for complete coordinate conventions, limits and Agent job workflow.

- 看不到对象：检查 Objects 的 Visible、相机 Fit 和 Section View 状态。
- 不能选择对象：确认对象可见，并检查当前工具是否只接受特定对象类型。
- 中文显示方框：确认 Windows 中文字体存在，重启 S33D 使字体回退重新加载。
- 操作按钮不可用：先按提示选择 region、surface、edge 或 object；必要时按 Esc 退出冲突工具。
- 布尔或 Chamfer 失败：检查对象是否相交、参数是否超过局部几何尺寸，并尝试较小数值。

2026-11-03：新增左栏“二维工程图 / 2D Drawing”，与 Sketch、Draw 3D 互斥；A4 三视图、孔/柱直径与中心间距等自动关键尺寸、逐对象尺寸明细页、三维选择预览、手动标注和 200 步标注撤销/重做。左栏、属性栏及 File 菜单使用统一 Export / 导出子菜单，支持 SVG、DXF、EPS 和多页 PDF，附带全部尺寸明细；DWG 暂不内置，可经 DXF 由另行许可的 CAD 转换。图纸及标注编辑随 .s33d 保存，重开后可继续编辑。详细步骤及网格投影限制见[中文手册](UserManual.zh-CN.md#24-二维工程图) / [English manual](UserManual.en.md#24-2d-engineering-drawings)。

2026-11-07：工程图普通文字、特征编号及标题栏文字支持拖动，特征引线保留原锚点，模型投影线锁定；尺寸数字仍可修改和拖动。新增 EPS 矢量逐页导出与内嵌字体文本；补齐斜向圆孔、圆柱端口的椭圆投影拾取、真实直径/半径和特征标注。文字位置支持撤销/重做及工程保存。

### 多孔草图区域选择（2026-11-09）

结束草图后，双击需要的材料区域；内部孔洞保持镂空。Objects 列表切换或追加其它对象、修改草图轮廓或平面后，原区域选择会清除，请重新双击选择。只有一个封闭区域时仍可直接使用拉伸。

相交轮廓按交点划分区域；不围成面的枝线、仅连接闭合圈的线段不作为材料边界，原线仍保留。无法通过几何验证时保留轮廓，属性栏持续显示错误，可展开“区域验证详情”；拉伸、切除、旋转及文字/齿轮/螺纹封盖停止该次生成，不提交缺面结果。修正轮廓后重新选择即可。


照片直接上传到 Agent 聊天，由 Agent 看图、调用 S33D 建模并对照原生模型图修正。S33D 参考照片导入是可选功能。首次连接及逐步操作参见 [PhotoToModel.md](PhotoToModel.md)。


工作区辅助显示与最近工程（11/19）：右上角“网格与 RGB 辅助轴”开关同步显示/隐藏网格及红绿蓝参考轴，与 View 菜单联动；Settings 的“重新启用所有隐藏提示”也会恢复两者，不影响模型/吸附/方向指示器。File > Open Recent（最近打开）按最近使用排序保留十个成功打开或保存的工程，悬停名称可查看完整路径，点击后沿用未保存确认和后台打开；列表随本机偏好保存，打开失败不改变当前工程。

关闭 S33D 会结束已连接的 MCP 桥接。重新打开后，如果 Agent 没有自动重连，请重启 Agent。连接入口为“设置 > Agent 连接 > 启用本机控制”。桥接工具与连接步骤详见 [AgentControl.md](AgentControl.md)。

当前 MCP 提供六个工具：`s33d_workflow`、`s33d_reference`、`s33d_render`、`s33d_describe`、`s33d_state` 和 `s33d_execute`。开始建模时先取工作流程与实时目录，再查询 `document.summary`；用原生模型图核对结果。安装程序提供连接模板，注册 Agent 仍需在本机明确操作。
