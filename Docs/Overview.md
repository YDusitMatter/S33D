# S33D 项目概览

更新：2026-11-22。本文概述项目定位与能力；使用方法见 [UserManual.md](UserManual.md)，几何后端构建与边界见 [ExactGeometryBuild.md](ExactGeometryBuild.md)。

## 项目定位

S33D 是一款现代、轻量、离线优先的本地 3D CAD 应用，面向个人设计、3D 打印和小型工作室的零件建模与多对象布局。用户可以通过桌面界面建模，也可以让外部 Agent 通过本机 MCP / CLI 接口操作同一个工程。

当前处于 **Alpha Preview** 阶段，版本为 **0.1.0-alpha.1**，作者 **YDZM**，项目源码采用 **MPL-2.0**。Windows 是主要开发与验证平台，其它桌面平台尚需独立验证。开发版功能已超过现有安装包；具体交付范围以版本及日志为准。

## 当前主要能力

### 草图与区域建模

- 在 XY / YZ / ZX、选中模型平面或线性方程定义的平面上创建 Sketch。
- 绘制直线、圆弧、圆、矩形、多边形、连续手绘和文字；支持位置、角度、尺寸参数编辑，以及关键点拖动、吸附和 Trim 修剪。
- Sketch 中支持 Pick 多选、Move / Rotate / Align，工作区及属性使用与 Gizmo 一致的世界 XYZ 坐标表达。
- 识别相交轮廓形成的最小封闭区域及孔洞，多选区域可 Extrude / Pocket / Revolve；文字的多个实心笔画可整段选取并拉伸。
- 新建草图记录逐笔撤销步骤，完成后仍可回选编辑；已支持的草图—实体—布尔—倒角依赖可事务式重建。

### 三维建模与对象管理

- Draw 3D 创建长方体、圆柱、球、圆锥和圆环，支持两次点击放置、实时参数反馈及回选编辑；可识别部分规则 Sketch–Extrude / Revolve 基础体。
- Extrude、Pocket（含贯穿）、Revolve、多体 Union、有序 Subtract / Intersect，以及 Mirror、Split、Chamfer / Fillet、Exact Offset / Shell。
- Move / Rotate / Scale 支持多对象；提供绝对位置、任意直线轴旋转、±90°、等比例百分比/目标尺寸缩放。
- Align 支持最小/中心/最大对齐、固定参考对象，以及拾取或方程定义的中心轴线对齐；另有 Copy / Cut / Paste、Array、Group / Ungroup 和批量显隐/颜色/删除。
- 有固定边界的选中平面可直接用于后续建模；完整圆柱外壁或内孔壁在支持条件下可整体选择并进入螺纹生成。
- 参数圆柱、精确实体及经认证的 Sketch Extrude/Pocket 完整柱壁支持直接改直径。分段草图实体转换为精确直接编辑结果，共享源草图保持不变；独立导入原始网格柱壁仍只读。

### 视图与交互

- 工作区单击棱、双击面/封闭区域、三击体，配合 Ctrl / Shift 多选；Objects 列表支持范围选择、隐藏项选择、编组、重命名和批量操作。
- 透视/正交、预设视角、Orbit / Pan / Zoom、Fit / Recenter、Gizmo、深度遮挡网格与坐标轴、橙色选择反馈及 Section View。
- 提供长度、面积、体积等测量反馈；显示单位支持 mm / cm / m / inch / ft，内部几何统一毫米。
- 中英文界面及独立语言 Help，四种外观：Simple / 简洁、Precision Classic、Graphite Studio、Porcelain Workshop；初始指引和设置记住用户选择。
- `.s33d` 工程保存对象、几何、特征参数、编组、相机、选择及 Undo/Redo；历史上限 200 步，工程替换/关闭前提供保存保护。
- 短窗口中的首次使用指引正文与齿轮、纹理、轮胎、轮毂参数可滚动，页脚操作保留；工具窗口拖小后重新分配空间。相机四向按钮宽度稳定，纹理预览限于预览框。逻辑布局及输入已有回归，真实跨 DPI/4K 显示仍需原生验收。
- 无边框工程图启动页和工作区按当前 Windows 显示器工作区、DPI 及边框适配；启动图按分辨率等比缩放并居中，首次完整绘制后至少保留两秒，启动文件加载较慢时延长显示。工程/网格加载与复杂建模显示后台阶段或读取进度，失败/取消不提交计算结果；尚无受控启动提速基准。

- Section View 对相交实体正常剖切，不相交或相切时保持完整；支持法线拖动与方程实时显示。数值工具首次激活聚焦第一个可用参数，Extrude/Pocket 拖动和深度框同步。
- File 的 Export… 使用系统保存对话框选择模型／工程图格式，读入与输出目录分别记忆；新增 3MF build 网格、单位、组件与 Production 引用导入。打印机／切片设置不导入，不支持的必需扩展明确报错。新外观偏好默认 Graphite Studio，保留已有选择。

### 方程、螺纹与插件

- Draw Equation 创建真实空间参数曲线（XYZ / 球坐标），以及有界 u/v 方程曲面；共面曲线可联合形成封闭区域，曲面支持相应加厚工作流。
- 螺纹工具提供公制、UNC / UNF、BSPP / BSPT 几何预设、打印建议及自定义参数，可对支持的内外圆柱壁批量加工。
- Thread generator 双列参数随可见空间展开，Warning / Correct / Generate 位于参数滚动区外；save as preset 打开独立命名窗口，避免保存表单占用页脚。Correct 按指定公称直径实际调整底孔/毛坯，保留螺纹和打印参数；修正与生成可分别撤销。局部加工后可重新选择完整剩余柱段，偏移从当前段起点计算。支持范围和操作见 [螺纹打印与修正](ThreadPrinting.md)。
- 七个内置插件：Scientific Calculator、Gear Generator、Surface Texture、Tire Generator、Wheel Rim Generator、Screw Generator、Nut Generator。
- 螺钉/螺帽生成器提供公称参考、自定义和打印粗牙配对、真实采样螺纹、径向间隙、入口倒角及保存/回编；尺寸参考和限制见 [FastenerGenerators.md](FastenerGenerators.md)。
- 插件通过清单发现，在菜单启用后出现紧凑工具栏，可展开/收起完整参数界面。Surface Texture / Tire / Wheel Rim **继续处于待改善状态**。

## 文件与工程互通

| 类型 | 当前支持 | 说明 |
| --- | --- | --- |
| `.s33d` | 保存、打开、另存 | 版本 2 工程保存当前态和撤销/重做；兼容读取旧版本 |
| STEP | 导入、导出 | Exact 输入保留精确 B-Rep；网格导出为合法平面 B-Rep，不能恢复原解析曲面 |
| IGES | 导入 | 支持可读取的实体及开放曲面 |
| STL / OBJ / PLY / GLB | 导入、导出 | 网格交换格式，不等同于精确曲面或完整工程 |
| SolidWorks 原生 / Parasolid | 暂不直接读取 | 需先外部转换为 STEP / IGES 等支持格式，见 [兼容说明](SolidWorksCompatibility.md) |

STEP / IGES 导入采用后台计算，成功后整批提交；支持通过文件关联或命令行路径打开文件。工程参数和历史使用 `.s33d` 保留，普通交换格式不保存全部应用状态。

## Agent 协作

S33D 桌面进程持有唯一的文档与几何状态，`s33d-agent` 提供 stdio MCP 和命令行桥接，通过本机回环连接控制正在运行的应用。Settings 提供服务开关、端口、测试以及 Codex / Claude Code 注册和取消注册。

当前接口覆盖状态查询、基础体/草图、拉伸/旋转体、变换、布尔、对象管理、文件/工程、历史/视图，以及方程、修剪、偏移和螺纹等已列出的命令。执行前以运行实例的 `system.describe` 为准；编组、中心轴对齐和紧固件配方生成/更新目前仍使用桌面工具，编组可从状态接口读取。

仓库内 `s33d-modeling` Skill 指导外部 Agent 从文字或参考图片澄清尺寸、确认方案、分步建模并核验结果。参考图片不能自动确定所有真实尺寸，S33D 本身也不内置图片理解或任意物体自动重建服务。接入方法见 [AgentControl.md](AgentControl.md)。

## 技术基础

| 层次 | 当前实现 |
| --- | --- |
| 语言与构建 | Rust / Cargo；精确后端包含 C++17 桥接及原生构建工具链 |
| 几何 | truck B-Rep + manifold-rust 网格运算 + OCCT 7.8.1 精确曲面/实体，多表示并存 |
| 桌面 UI / 渲染 | eframe / egui + wgpu 自定义三维视口 |
| Workspace | `s33d-kernel`、`s33d-model`、`s33d-app` 三个 crate；Agent 桥接属于 app crate 的另一个可执行目标 |
| 数据约定 | 内部毫米、Y-up；工程 JSON 保存参数、网格及可选精确 B-Rep |
| 验证 | 单元、集成、几何/工程回归和 headless UI 检查；Windows 本地构建验证 |

几何操作按输入表示分派；`GeometryKernel` 是公共接口，但应用仍使用部分具体内核扩展，尚未做到任意后端完全替换。精确几何后端的依赖、构建与限制见 [ExactGeometryBuild.md](ExactGeometryBuild.md)。

## 当前边界与后续重点

- **参数化范围：**已有部分依赖重建及参数回编，不是完整的通用特征树、约束求解器或机械装配约束系统；测量反馈也不等于完整二维工程图标注。
- **几何适用范围：**锐边 Chamfer / Fillet 仍有网格刀具路径；Offset / Shell 需要 Exact 输入，冻结输入不会自动跟随上游。复杂尖角、自交和极端参数可能失败。
- **拓扑引用：**Body ID 可稳定使用；一般面/棱的长期稳定命名仍待实现，不能把当前面序号当永久引用。
- **性能与可靠性：**已完成原生优化、轻量预览及缓存等第一阶段改进；复杂原生运算仍可能耗时。取消主要丢弃结果，尚无原生强取消或进程级故障隔离。
- **制造与插件：**采样螺纹/紧固件仍需真实打印和制造配合验收，未提供公差等级、量规或强度认证；三个待改善插件继续完善真实几何与规格。
- **交付：**继续完善复杂导入/交互、本地化、跨平台及干净 Windows/GPU 验收，完成正式品牌图标和后续安装包。原有安装包不自动包含最新开发版功能。

截至 2026-11-10，最新完整 workspace 回归为 **526 passed / 0 failed / 14 ignored**；忽略项为既有本机夹具/手动测试。本日补充覆盖真实 Sketch Extrude/Pocket 缩扩孔、斜向/对称/反向拉伸、保存重开和螺纹生成，以及双语面板的真实指针点击。开发版已更新，现有安装包仍为较早版本；本轮文档收尾不重打安装包或发布。功能边界见 [用户手册](UserManual.zh-CN.md)，内部证据另存本地收尾记录。现有测试不替代上述待验收项目。

项目源码许可及第三方材料分别见根目录 [LICENSE](../LICENSE) 与 [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md)。
