# 资产存储：内容寻址对象库 + 硬链接 release 树

> 回答两个问题：**当前每个游戏版本的数据是怎么存的**，以及**相近版本大量重复时如何省空间**。
> 所有数字来自 2026-11-05 本机实测（`release/asset_pack` 等 manifest）。

## 一、当前做法（2026-11-05 之前）

```
/srv/wotb-assets/
  releases/<release-id>/        # 不可变的完整逻辑树（index.json / manifest.json / glb / map / ...）
  datasets/<dataset-id> -> ../releases/<release-id>
  current -> datasets/<default-id>
  versions.json
```

- **每个 release 是一份完整目录树**；`manifest.json` 记录全部文件的 `path/bytes/sha256`，
  `delta.json` 记录相对上一 release 的变更。
- 发布新 release：`cp -al releases/<base> releases/<new>` 先硬链接复制（未变文件共享 inode），
  变更文件用 `cp --remove-destination` 覆盖（断链），再在主机重算 manifest、原子切 dataset 软链。
- 因此**同一 lineage 内**未变文件确实只占一份盘；同内容重建/回滚不复制数据块。
- 局限：
  1. 去重是**lineage-local**：新 dataset 若从不同 base 构建，跨 dataset 的相同文件不会共享 inode。
  2. **同一 release 内**同一内容出现在多个路径时（例如不同变体目录/重复导出），manifest 是
     两个条目、文件系统上若未预先硬链接就是两份数据。
  3. `cp -al` 只解决“从 base 复制”这一步；独立生成的包（不同版本、不同工具批次）之间没有全局去重。
  4. 每个 release 仍有一份完整 `manifest.json`（约 0.85 MB）和目录项；相对 4.4 GB 内容可忽略，
     但 release 数量增长时目录项/inode 会累积。

### 实测重复量

| 对比 | 结果 |
|---|---|
| 11.21 单包内部 | 5965 条路径 / 逻辑 **4432.9 MB**；5841 个不同 sha256 / 唯一 **4216.7 MB** → 包内重复 **4.9%**（124 条重复路径，216 MB） |
| 11.21 vs t11_preview | 5907 条共同路径，其中 **5899 条内容相同**（99.86%），仅 8 条不同 |
| 3 个本机包合计 | 逻辑 **13075.9 MB** → 全局唯一内容 **4237.4 MB**，去重潜力 **67.6%** |

结论：相近版本的大部分文件是逐字节相同的；值得把内容收敛到一份对象库，而不是让每个 release
各自持有一份物理副本。

## 二、目标布局（内容寻址 + 硬链接树）

```
/srv/wotb-assets/
  objects/
    <sha[:2]>/<sha256>          # 唯一内容；文件名 = 内容 sha256；每个唯一 blob 一个 inode
  releases/
    <release-id>/...            # 硬链接到 objects；目录真实存在
  datasets/<dataset-id> -> ../releases/<release-id>
  current -> datasets/<default-id>
  versions.json
  release-log.jsonl
```

- **对象库保存唯一内容**：同一份字节无论出现在多少个 release、多少个 dataset、多少个路径，
  对象库里只有一个 inode。
- **release 树是硬链接视图**：`releases/<id>/<path>` 与 `objects/<sha[:2]>/<sha>` 同 inode；
  客户端/Caddy 仍按普通静态文件读取，无需动态服务、无需改 URL。
- 空间 = 唯一内容 + 每个路径的目录项/inode 元数据；重复内容不占额外数据块。
- `manifest.json` 的逐文件 sha256 仍是完整性权威；对象名只是内容身份。`version.json` /
  `delta.json` / `manifest.json` 是小体积元数据，按 release 保留普通文件。

## 三、工具：`tools/asset_store.py`

```bash
# 1) 把一份完整 pack 的内容登记进对象库（默认硬链接，同盘零拷贝）
python tools/asset_store.py import --pack release/asset_pack --objects /srv/wotb-assets/objects

# 1b) 只上传了变更文件时：manifest 仍用完整清单，源文件根只放变更文件
python tools/asset_store.py import --manifest incoming/manifest.json \
    --files incoming --objects /srv/wotb-assets/objects

# 2) 从 manifest + objects 构建 release 树（硬链接；metadata 目录提供 version.json/delta.json）
python tools/asset_store.py materialize --manifest incoming/manifest.json \
    --objects /srv/wotb-assets/objects --out /srv/wotb-assets/releases/<new> \
    --metadata incoming

# 3) 迁移/去重已有 release：原地把文件改成对象硬链接（内容不变，manifest 不变）
python tools/asset_store.py adopt --releases /srv/wotb-assets/releases \
    --objects /srv/wotb-assets/objects --dry-run
python tools/asset_store.py adopt --releases /srv/wotb-assets/releases \
    --objects /srv/wotb-assets/objects

# 4) 统计/校验
python tools/asset_store.py stats --objects /srv/wotb-assets/objects \
    --releases /srv/wotb-assets/releases
python tools/asset_store.py verify --objects /srv/wotb-assets/objects \
    --releases /srv/wotb-assets/releases --links --full

# 5) prune 之后回收无引用对象（先清理 staging pack；--dry-run 先看）
python tools/asset_store.py gc --objects /srv/wotb-assets/objects \
    --releases /srv/wotb-assets/releases --dry-run
python tools/asset_store.py gc --objects /srv/wotb-assets/objects \
    --releases /srv/wotb-assets/releases
```

命令语义：

- `import`：对象已存在→复用（不要求源文件存在）；对象不存在→从 `--pack` / `--files` 导入；
  两者都没有→fail-closed 列出缺项。`--copy` 用于跨盘 staging；同盘默认 `os.link`。
- `materialize`：输出目录必须不存在/为空（`--force` 才清空）；每个 manifest 条目必须命中对象；
  输出文件是对象硬链接；`manifest.json` 与 `--metadata` 里的 `version.json`/`delta.json` 复制。
- `adopt`：把已有 release 树里的文件原地替换为对象硬链接；内容与 manifest 不变；用于迁移或
  把独立生成的 release 收敛到全局对象库。
- `gc`：只删除“没有任何保留 release 的 manifest 引用 **且** `st_nlink == 1`”的对象。
  仍在 staging/其他硬链接里的对象不会被删；因此 gc 前先清理 `incoming-*` staging。
- `stats`：唯一内容 vs 各 release 逻辑总量 vs 去重率。
- `verify --links`：要求 release 文件与对象同 inode；`--full` 对每个唯一对象重算 sha256。

## 四、发布 recipe

### A. 沿用现有 `cp -al` 流程（低风险迁移）

1. 按 §5.2 构建新 release（`cp -al` + `cp --remove-destination` 覆盖 + 主机重算 manifest）。
2. `asset_store.py adopt --releases releases/<new> --objects objects`（把新 release 收敛到对象库；
   未变文件与全局对象命中，跨 dataset 也去重）。
3. `deploy/assets/dataset.sh link --dataset <id> --release <new>`；`verify`。
4. prune 旧 release 后 `asset_store.py gc`。

### B. 对象原生发布（省去完整树复制）

1. 本地打包得到完整 `manifest.json` + 变更文件；上传到 `incoming-<batch>/`（保留相对路径）。
2. `asset_store.py import --manifest incoming/manifest.json --files incoming --objects objects`
   —— 未变文件命中已有对象，只导入变更内容。
3. `asset_store.py materialize --manifest incoming/manifest.json --objects objects \
   --out releases/<new> --metadata incoming`
4. `dataset.sh link --dataset <id> --release <new>`；`verify --links`。
5. 清理 `incoming-<batch>/`；prune 后 `gc`。

## 五、迁移步骤（一次性）

1. 对当前 release 跑一次 `import --pack releases/<current> --objects objects`（硬链接，零数据复制）。
2. `adopt --dry-run` 检查影响面，再执行 `adopt --releases releases --objects objects`：
   所有已有 release 原地收敛，重复内容共享 inode；release 内容/manifest 不变。
3. `stats` 记录唯一内容/逻辑总量；`verify --links --full` 抽查/全量校验。
4. 之后新发布按 §四 A 或 B；`gc` 只在 prune + staging 清理后执行。

## 六、运维注意

- **同一文件系统**：`objects` 与 `releases` 必须同盘，否则硬链接失败；跨盘 staging 用
  `import --copy` 写入对象库，materialize 仍要求同盘。
- **备份**：必须保留硬链接（`rsync -aH` / GNU tar 默认硬链接记录）。普通 `cp -r`、无 `-H` 的
  rsync 会把对象展开成多份，备份体积会回到逻辑总量。
- **Caddy**：对象库不对外；只伺服 `releases` 树。硬链接对 `file_server` 透明，ETag/内容语义不变。
- **并发**：import/materialize/adopt/gc 与 `dataset.sh` 共用 host 发布锁（`flock
  /srv/wotb-assets/.publish.lock`）；gc 的 `--min-age-seconds` 是额外保险。
- **GC 与 staging**：`st_nlink > 1` 的对象不删；若 staging pack 未清理，对象会保留（不占额外
  数据块，但对象目录项会留下）。清理 staging 后再 gc。
- **硬链接上限**：ext4 单 inode 链接数上限约 65k；release 数量远低于此。对象按 sha 前 2 位分片
  （256 个目录），避免单目录过大。
- **不可变语义**：release 内文件仍不得原地写；覆盖必须先断链（`cp --remove-destination` /
  临时文件 + `os.replace`），否则会同时改写其他 release/对象。

## 七、收益与生产实测

- **生产迁移（2026-11-05）**：`/srv/wotb-assets` 的 9 个 release 逻辑 **39449.5 MB** →
  全局唯一 **6107 个对象 / 4237.9 MB**，去重 **89.3%**；`verify --full --links` PASS
  （53035 条路径、6107 个对象全量哈希），release 树在盘上 **4.0 GB**。迁移只重排硬链接，
  未改动任何文件内容，URL/ETag 语义不变。**超测 r4 发布后（2026-11-06）**：**18 个 release**，
  **6899 个对象 / 4273.1 MB（去重 94.6%）**，逻辑 **79536.9 MB**，盘上 **4.1 GB**（新 release 经
  `import --manifest` + `materialize` 以硬链接入库存放；r4 相对 r3-xi-upgrades 仅 14 件变化，
  新增对象 +8）。
- 单包内部重复：约 **4.9%**（11.21 本机实测 216 MB）。
- 相近版本/不同 dataset 的重复：按 3 个本机包实测，逻辑 **13075.9 MB** → 唯一 **4237.4 MB**，
  约 **67.6%** 的数据块可省（具体取决于实际 release 集合与 lineage）。
- 目录项/inode 仍按 release 数量线性增长（每个路径一个 dentry）；相对 4.4 GB 数据量很小，
  若未来 release 数量极大，可再考虑 symlink 树或动态 CAS 解析。