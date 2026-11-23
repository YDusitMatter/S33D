# S33D tool icons — provenance and limited IP review

Review date: 2026-10-30. Scope: the 45 tool pictograms in `src/ui_icons.rs`,
shared by the three optional appearances. Original appearance remains available.

**Result: source and asset audit completed; worldwide patent/design/trademark
clearance has NOT been established.** No specific conflicting right was
established by this limited review. That is not a finding that none exists.

## 来源与许可

这些图标是本项目开发过程中通过 AI 辅助编写的坐标绘制代码：线段、
矩形、多边形、椭圆和箭头由 egui Painter 在运行时绘制。
本次源代码和资产检查没有发现复制或加载第三方图标包、图标字体、
竞争产品 Logo、截图裁切图标或外部 SVG 路径；`Cargo.toml` 未引入图标库。
此说明记录已检查到的来源，不保证生成内容与所有在先图形不存在相似。

本轮审查版本的 `src/ui_icons.rs` SHA-256（含来源注释）：
`a8484858e3eaaacf860581dddd7e42aa847ea13f5b57348974312c47ce80f375`。
后续改图时应重新审查，不能把本次记录自动套用于新版本。

`src/ui_icons.rs` 随本项目源码采用根目录 `LICENSE` 的 MPL-2.0。
这是源码许可说明，不是对每个基础几何符号的独占权声明，也不保证
AI 辅助生成内容本身具有可登记的版权。不将本模块错误列为第三方图标包。
egui 和文字字体仍按 `THIRD_PARTY_LICENSES.md` 中各自条款使用。
MPL-2.0 不提供不属于贡献者的第三方专利许可，也不授予贡献者的商标权。
见 [MPL-2.0 第 2.1、2.3 条](https://www.mozilla.org/en-US/MPL/2.0/)。

## 逐项图形检查

以下检查针对源代码路径及生产路径导出的 18 / 36 px 联系表。
结论均为“功能示意、未见明显品牌标识”；不是逐项商标数据库或专利比对结论。

| 图标 | 所表达的功能图形 |
| --- | --- |
| Move | 中心向四方的移动箭头 |
| Rotate | 围绕中心点的弧形箭头 |
| Scale | 小矩形、角标和放大方向 |
| Align | 参考线与两个对齐矩形 |
| Sketch | 平面与绘制笔 |
| Pick | 选择光标 |
| Trim | 剪刀与待裁线段 |
| Line | 两端点之间的直线 |
| Arc | 带端点的四分之一圆弧 |
| Circle | 圆与中心点 |
| Rect | 矩形及对角关键点 |
| Poly | 多边形及顶点 |
| Pen | 自由笔画与绘制笔 |
| Text | 由几何线条组成的 T |
| Exit | 开口边框与退出箭头 |
| Draw3D | 立方体与添加标记 |
| Box | 轴测立方体 |
| Cylinder | 圆柱的椭圆端面和侧边 |
| Sphere | 球体轮廓与经纬截线 |
| Cone | 锥体顶点、侧边与椭圆底面 |
| Torus | 圆环体的外环与内孔 |
| Equation | 坐标轴与采样曲线 |
| Extrude | 轮廓形成棱柱及向外箭头 |
| Pocket | 开放凹腔及向内箭头 |
| Revolve | 旋转轴、截面与环绕箭头 |
| Union | 重叠矩形的并集区域 |
| Subtract | 重叠矩形的差集区域 |
| Intersect | 重叠矩形的交集区域 |
| Split | 分开的两部分与切分虚线 |
| Section | 物体剖面与剖线 |
| Mirror | 对称虚线两侧的镜像轮廓 |
| Fillet | 具有圆弧过渡的转角 |
| Chamfer | 具有斜切面的转角 |
| OffsetShell | 内外壁厚截面与偏移箭头 |
| Copy | 两张重叠的矩形页 |
| Paste | 剪贴板与矩形内容 |
| Delete | 垃圾桶 |
| Array | 规则排列的四个矩形 |
| Objects | 树状层级与对象行 |
| Iso | 轴测立方体 |
| Top | 立方体的顶面填色 |
| Front | 立方体的前面填色 |
| Right | 立方体的右面填色 |
| Fit | 取景角标与居中矩形 |
| Recenter | 十字方向线与中心点 |

相同基础几何不等于当然侵权，也不等于当然不侵权。图形在具体界面中的
整体视觉效果和使用方式仍可能重要。此次没有找到支持定向改画的具体冲突，
因此保留可识别的功能图形，不以随意改线宽、换颜色或旋转图形冒充法律规避。

## 已采取的措施

- 所有新工具保留明确文字标签，不以这些工具图形替代 S33D 产品身份。
- 不从概念图、竞争产品截图或网络图标库提取生产素材。
- 图标源码添加 SPDX 许可标识与本记录链接。
- UI 与相机的两处旧“竞品风格”注释改成中性功能描述。
  这是避免无谓品牌关联的编辑，不是认为旧注释本身构成侵权。
- 三张 AI UI 概念 PNG 已加入 `.gitignore`；只保留在本机作设计讨论。
  现有安装包和源码包脚本的显式文件白名单本来也不包含 `output/`。
  `icons-sheet.svg/png` 是项目绘制路径导出的检查表，可保留作证据。
- `assets/icons/s33d-seed.*`、`s33d-sprouted.*` 与这 45 个工具图标分开记录。
  2026-11-12 用户批准迭代后的铁核桃图案：银色管道 S3 和叶片、铜色 3D，
  右侧壳体补圆、保留裂缝，根通过标准管接到银色 3 的下端。
  当前母版为 `assets/icons/s33d-artwork.png`，原始附件、带来源元数据的
  已批准生成图与旧版资源留在本地备份。转换脚本为 `scripts/update-brand-icons.ps1`，
  尺寸、角色与校验清单见 `assets/icons/README.md`。旧文件名保留安装器兼容。
  该替换只记录素材与使用授权，不表示已完成名称或 Logo 的商标清查。

## 公开资料核查与边界

本轮进行了源代码/资产检查、联系表目视检查和有限公开网页检索。
实际检索词包括 `CAD icon extrude`、`Shapr3D icon`、
`Autodesk display screen icon` 及中国/美国 GUI、商标审查规则。
关键词网页检索不能替代图形分类、申请人别名、专利族和有效法律状态检索。

公开检索返回 Autodesk 的美国外观专利
[D624933](https://patents.google.com/patent/USD624933S1/en)、
[D625327](https://patents.google.com/patent/USD625327S1/en)、
[D625328](https://patents.google.com/patent/USD625328S1/en) 等
图标条目；D624933 的公开页面描述与 3D 视图导航有关。
部分页面或图纸未能通过本次工具完整读取，未作逐幅图纸比对，也未在
官方案卷中确认有效状态。**不得将这些条目写成已确认侵权或已排除冲突。**
条目题名或聚合网站的 Expired 标签均不足以对全部现行权利作结论。

| 权利问题 | 本次可确认的事项 | 仍未完成的事项 |
| --- | --- | --- |
| 第三方资产/版权来源 | 未发现引入第三方图标资产；代码来源、许可和检查表已记录 | 不保证 AI 辅助生成图形与所有既有作品无相似 |
| 商标及来源混淆 | 45 图标未见明显竞争品牌标识；用于功能按钮 | 未逐项执行商标图形库、未注册使用及各销售地清查；名称、Logo、主题名也未取得可注册结论 |
| 外观设计/设计专利 | 中国、美国都存在 GUI / 图标外观保护制度 | 未完成有效专利/注册外观的逐项图纸、专利族、地域与期限比对 |
| 发明/实用专利及界面整体 | 此次不改变建模算法与操作流程 | 本次不构成全程序算法、交互方法或整体 GUI 的 FTO 意见 |

中国的官方指引及审判资料说明 GUI 可以涉及外观设计保护；不能因为
产品是软件就忽略这一类权利。见
[CNIPA：涉及图形用户界面的产品外观设计专利申请指引（2025-12-05）](https://www.cnipa.gov.cn/art/2025/12/5/art_66_202950.html)、
[最高人民法院：GUI 外观设计保护案例](https://www.court.gov.cn/zixun/xiangqing/452031.html)。

美国对计算机界面/图标外观申请有专门规则，2026-03-13 又发布补充指引。
见 [USPTO 2026 年补充指引](https://www.federalregister.gov/documents/2026/03/13/2026-04987/supplemental-guidance-for-examination-of-design-patent-applications-related-to-computer-generated)。
独立编写绘图代码和使用通用功能含义，不能单独证明不存在专利侵权。

商标审查关注相似标识在相同或相关商品/服务上是否导致来源混淆，
不只看是否完全相同。完整检索也需要考虑未注册使用。见
[USPTO：混淆可能性](https://www.uspto.gov/trademarks/search/likelihood-confusion)、
[USPTO：完整商标清查](https://www.uspto.gov/trademarks/search/comprehensive-clearance-search-similar-trademarks)。
图形检索可结合设计分类代码和图像相似检索；官方说明两者应互补。
截至本次检查，图像检索需要登录；本次未登录执行图像检索，也未上传本项目图标，
不将本轮网页搜索描述成已完成官方图像数据库清查。见
[设计分类代码](https://www.uspto.gov/trademarks/search/design-search-codes)、
[图像检索现行说明](https://www.uspto.gov/trademarks/search/trademark-search-system-updates)。

## 后续发布时的处理规则

若需要正式商业发布的法律确认，应以最终发布图标及其完整界面、
实际销售国家/地区、用途和时间为范围，交由相应司法辖区的知识产权
专业人员做完整权利清查。保留源代码与历史记录；若收到具体权利号或
具体相似图形，再根据其有效权利范围定向替换相关图标。

不得把本文件、MPL-2.0 或“原创/AI 绘制/通用几何”用作“零侵权风险”保证。
本轮没有改变图形路径、主题外观、模型、操作逻辑或用户设置，因而不需要
关闭用户当前正在绘制的工程或重新启动应用。

## 2026-11-03: engineering-drawing pictogram

Added `EngineeringDrawing`: a page rectangle with two projection symbols and a dimension line. Its paths were authored here from basic geometry; no external SVG, font glyph or vendor mark was used. The new source hash is `47f302763077bb397f68f258bbd2913288f6fbd32bb5c04dfcdccd8e5e0e95ac`. This is a provenance record only and does not extend the earlier limited legal review to new content. Geometry bounds and distinctness are checked by the existing icon tests.
