# 紧固件生成器 / Fastener generators

2026-10-31，实施者 Codex。使用方法已集成双语 Help：
[中文](UserManual.zh-CN.md#编组中心轴线对齐与紧固件插件) · [English](UserManual.en.md#groups-center-axis-alignment-and-fastener-plug-ins)。

## 尺寸基准 / Dimensional references

公称公制预设包括 M3、M4、M5、M6、M8、M10、M12；头部和螺帽尺寸采用下表。单位 mm。它们是建模起始值，不包含制造公差、强度等级或量规认证。螺钉初始长度为 3d，支持自定义。所有尺寸可编辑。

Nominal metric presets use the following starting dimensions in mm. They do not include manufacturing tolerances, property classes or gauge certification. Default screw length is 3d and remains editable.

| Size | Pitch | Hex flats | Hex head height | Socket cap diameter / height | Socket flats | Nut height |
|---|---:|---:|---:|---|---:|---:|
| M3 | 0.5 | 5.5 | 2 | 5.5 / 3 | 2.5 | 2.4 |
| M4 | 0.7 | 7 | 2.8 | 7 / 4 | 3 | 3.2 |
| M5 | 0.8 | 8 | 3.5 | 8.5 / 5 | 4 | 4.7 |
| M6 | 1 | 10 | 4 | 10 / 6 | 5 | 5.2 |
| M8 | 1.25 | 13 | 5.3 | 13 / 8 | 6 | 6.8 |
| M10 | 1.5 | 16 | 6.4 | 16 / 10 | 8 | 8.4 |
| M12 | 1.75 | 18 | 7.5 | 18 / 12 | 10 | 10.8 |

基准：六角头参考 ISO 4017，内六角圆柱头参考 ISO 4762；M5–M12 六角螺帽高度参考 ISO 4032:2023，M3/M4 使用旧 DIN 934 高度。沉头、手拧、方帽、圆帽、法兰帽是自定义形状，并未宣称符合对应完整产品标准。标准型也采用理想化头形、简化内六角深度及采样螺纹，不包含标准要求的全部倒角/圆角。

Hex heads reference ISO 4017; socket caps reference ISO 4762. M5–M12 regular nut heights reference ISO 4032:2023; M3/M4 use legacy DIN 934 heights. Countersunk/thumb screws and square/round/flange nuts are custom starting shapes. Even the reference styles have idealized heads, simplified sockets and sampled threads rather than every standard chamfer or fillet.

查询来源 / Sources checked:

- [ISO 4017 standard text](https://sepandpitch.com/wp-content/uploads/2023/06/iso-4017.pdf).
- [Accu ISO 4762 M6 datasheet](https://www.accu.co.uk/api/product-datasheet?id=2874), [M10 datasheet](https://www.accu.co.uk/api/product-datasheet?id=4058).
- [ISO 4032:2023 reference](https://standards.iteh.ai/catalog/standards/iso/1d8f87aa-0388-4a7c-a4a5-802a81b2555a/iso-4032-2023), [Würth ISO 4032 M12](https://eshop.wurth.nl/Hexagon-nut-ISO-4032-steel-6-8-plain-NUT-HEX-ISO4032-8-WS18-M12/0310912.sku/en/US/EUR/).
- [DamenCNC DIN 934 M3](https://www.damencnc.com/en/m3-nut-din934/a3711).

## 几何和打印 / Geometry and printing

- 真实 60° 螺旋切削网格；外牙径向深度 0.61343P，内牙 0.541266P，截顶/底平段按现有螺纹模块计算。内外牙槽相位相差半圈以匹配牙峰。
- 打印间隙通过整体径向平移牙形实现，不加深或加宽刀具；单件径向值为 c，螺钉公称半径减 c，螺帽孔半径加 c。两件均设 c 时总径向余量约为 2c。
- Print coarse 12 × 2：d=12、P=2、c=0.15、入口倒角 0.6；螺帽高 12、对边 22，螺钉头宽/直径 24。推荐先打印短试件，不承诺特定设备可直接旋合。
- 最多 125 圈。生成在后台进行，成功一次提交；失败或文档改变丢弃结果。保存等待完成，不把未生成几何当作已保存。
- 保留可编辑配方和线性位姿，支持保存/重开、撤销/重做、移动/旋转后更新。最终为网格实体，适合 STL 导出，不是解析 OCCT 螺纹 B-rep。

The generator cuts real sampled 60-degree helices. Male/female depths are 0.61343P/0.541266P, with complementary groove phase. Print clearance shifts the complete radial profile without changing flank angle or pitch; two parts each using clearance c provide approximately 2c combined radial clearance. Print coarse 12 x 2 is a custom printable starting point, not a standardized tolerance class. Always print a fit sample. Jobs are limited to 125 turns and commit atomically. Recipes and placement remain editable and serializable; outputs are mesh solids suitable for STL, not analytic OCCT threads.

未引入新依赖、字体、第三方图标或模型素材。No new dependencies, fonts, third-party icons or model assets are introduced.
