# SolidWorks compatibility / SolidWorks 兼容说明

Verified against the S33D source and OCCT 7.8.1 build on 2026-10-30.

| Format | Current S33D behavior / 当前行为 |
| --- | --- |
| STEP `.step`, `.stp` | Preferred CAD exchange; native B-rep import/export. 推荐交换格式，原生 B-rep 导入/导出。 |
| IGES `.igs`, `.iges` | Native B-rep/surface import. Files containing unsewn surfaces remain surfaces, not invented solids. 原生曲面导入；未缝合曲面不会被假装成实体。 |
| STL, OBJ, PLY, GLB/glTF | Mesh import. No recovery of original analytic surfaces or SolidWorks features. 网格导入，不恢复原解析曲面或特征。 |
| `.sldprt` | Native SolidWorks part; separate translator required. 原生零件，需要独立转换器。 |
| `.sldasm` | Native assembly; separate translator plus referenced components required. 原生装配，需要独立转换器及关联零件。 |
| `.slddrw` | Drawing, not a 3D body. Obtain the corresponding part/assembly. 工程图不是三维实体，请取得对应零件/装配。 |
| Parasolid `.x_t`, `.x_b` | Not supported by the current open-source OCCT build; separate translator required. 当前开源内核构建不支持，需要独立转换器。 |

## Transfer a part or complete assembly / 转移零件或完整装配

1. Open the source in SolidWorks, resolve referenced/lightweight components, and choose the desired configuration. 在 SolidWorks 打开源文件、解析关联/轻量化零件并选择需要的配置。
2. File > Save As > STEP (preferred) or IGES. Export the complete assembly together to retain relative positions. 文件 > 另存为 > STEP（推荐）或 IGES；整组导出装配以保留相对位置。
3. In S33D, File > Import Model. STEP/IGES run in a cancellable background job. S33D places the complete model once, then splits solids and loose surfaces into objects. 在 S33D 导入模型；后台读取可取消，整组定位后拆分实体及独立曲面。
4. Check overall dimensions, object count and orientation before editing. Lengths transfer in millimetres, independently of S33D's display-unit setting. 编辑前检查尺寸、对象数量和朝向；内核以毫米传递长度，不受显示单位影响。

S33D currently imports geometry and relative placement, not mates, configurations, a feature tree, or original assembly names/materials. IGES wire-only data cannot become a solid automatically. The application does not install or invoke a proprietary converter silently.

当前保留几何和相对位置，不恢复配合、配置、特征树及原始装配名称/材质。只有线框的 IGES 不会自动变成实体；程序不会静默安装或调用专有转换器。

## Evidence / 依据

- Local source: `crates/kernel/native/occt_bridge.cpp`, `crates/kernel/src/occt.rs`, `crates/app/src/exact_tools.rs`.
- [SolidWorks: exporting documents and setting options](https://help.solidworks.com/2025/english/SolidWorks/sldworks/t_Exporting_Documents_Setting_Options.htm?id=18.10).
- [SolidWorks: STEP export options](https://help.solidworks.com/2025/english/SolidWorks/sldworks/HIDD_EXPORT_OPTION_STEP.htm).
- [SolidWorks: IGES export options](https://help.solidworks.com/2025/english/SolidWorks/sldworks/HIDD_EXPORT_OPTION_IGES.htm).
- [Open Cascade: built-in formats and separate commercial components](https://github.com/Open-Cascade-SAS/OCCT/discussions/1157).
