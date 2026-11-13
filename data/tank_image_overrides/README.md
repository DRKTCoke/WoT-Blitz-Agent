# XI 级预览车封面

本批 8 张透明 WebP 是用 WG 预览模型及默认贴图渲染的非官方车图。XM69 HACKER 的客户端封面用于校准机位和占幅，其原图由客户端导出器直接提供。车辆模型和美术权利属于 Wargaming；本项目与 Wargaming 无隶属关系，亦未获其背书。使用这些图片需符合 EULA 和 [Wargaming Player Content Policy](https://legal.wargaming.net/en/user-documents/content-policies/player-content-policy)。仓库代码许可证不涵盖游戏美术。

[清单](manifest.json) 给出源客户端 ZIP、生成用 release、占位图及逐车输入和输出 SHA-256。每辆的外观输入是该 release 的 `glb/{id}/model.glb` 与 `tank/{id}.json`，原图来自 `tank_images/{id}.webp`。使用模型的默认活动场景和配置里的炮塔、主炮。相机方位角 30°、仰角 39°，透明画布 256×128，保留天线与炮口。此批素材未使用第三方宣传照或 AI 生成图。

## 从已解包的原始资产复现

普通资产构建直接使用已审图，无需浏览器。需要重渲时，使用清单里记录的 Three.js、Playwright、Chromium 内核与 Pillow 版本，准备同一来源的模型与车辆配置。在 Agent 仓根目录运行：

```bash
python data/tank_image_overrides/render/render.py \
  --pack "$RAW_PACK" --three "$THREE_PACKAGE" --browser "$CHROME" --out "$RENDER_OUT" \
  10593 15153 23361 27905 28161 30241 30737 30993
```

核对结果与清单的 `sha256`。换渲染器或模型后需要重新审图。批准新的输出时一起更新 WebP 和清单。

## 生成与增量更新

全量构建仍从 `data/cache/tank_images` 读取 WG 导出的原图，增量更新使用 `tools/apply_data_version.py`。手工重建同一超测包时，在一份完整、尚未 stamp 的候选目录运行下面的命令。原图目录须来自该次 WG 导出；上一次的覆盖产物不能用作原图。

```bash
python scripts/export_asset_pack.py --refresh-manifest --out "$CANDIDATE" \
  --data "$CANDIDATE/data" --dataset-id 11.4.0.138 --raw-tank-images "$RAW_ICONS"
python tools/asset_release.py verify --pack "$CANDIDATE" --full  # 在现有 stamp 步骤之后
```

这里的 dataset ID 是 WG 源版本。发布时可用独立版本路径，确保客户端同时切换图片与 `crops.json`。命中车型、原图、模型和配置的校验值，才使用已审封面。原图改变时本次构建立即保留新原图；复核为正式图后将清单条目设为 `enabled: false`。替换文件缺失或校验失败也回退至原图并记录原因。最后从实际输出重算裁剪框和包校验清单。
