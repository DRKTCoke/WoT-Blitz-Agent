# AGENTS.md

## No statistical-judgment fixes (2026-10-14 user directive)

Solutions must never decide per-case semantics through measured-distribution heuristics
(thresholds on brightness/percentage/void-signature scans, "detect then patch" detectors, or
any statistics-flavored classification of individual data items). Semantics come from the
format's authoritative definition (engine/tool source, file-format invariants) and apply
**uniformly to every item of the same class**: if a class of data needs a transform, the same
transform applies to all of it; if only a subset seems affected, find the semantic marker that
defines that subset (a file-level flag, format version, distinct structure) — do not infer the
subset from content statistics. When no semantic discriminator exists, fail closed and report
instead of guessing.

## Release policy

`Cargo.toml` → `[workspace.metadata.release].version` is the single source of truth for WoT-Blitz-Agent / WASM release versions.

- Keep `[workspace.metadata.release].version` as plain SemVer `x.y.z` without a leading `v`.
- Normal feature/fix PRs must not bump `workspace.metadata.release.version` unless that merge is intentionally meant to publish a release.
- A release-intended PR must bump `workspace.metadata.release.version` and update the matching release entry in `docs/index.md`.
- When `workspace.metadata.release.version` changes on `main`, `.github/workflows/release.yml` automatically runs tests, builds the WASM package, creates tag `v<version>`, and publishes the GitHub Release.
- Do not manually create or move `v*` tags or GitHub Releases in the normal release flow.
- Released tags are immutable. Never reuse an existing version for a different commit.
- `Cargo.toml` package versions are not the product/WASM release version; do not change them just to publish a GitHub release.
- Release artifacts must be named `wotb-replay-wasm-v<version>.zip`, and `fingerprint.json` must record the exact release tag and upstream commit.
- If a release job fails after tag creation, rerun the same workflow/commit instead of creating another tag.

## Getting CI results (no sleep polling)

**等待姿态（2026-10-15 用户指令）：一律**放后台进程**盯结果（工具层 `run_in_background`），
当前回合继续别的工作或直接收尾；后台任务结束时以完成通知回来。**不要阻塞等待**
（`--watch` 不加 `run_in_background` 会把整个回合挂住）。"继续 / 等它跑完"这类措辞也按此执行。**

- Live PR status: `gh pr checks <pr-number> --watch` (prints one line per check as its state changes; works in non-TTY/background).
- One run's verdict: `gh run watch <run-id> --exit-status --compact` (blocks to the end; when piped/backgrounded it prints the summary only at completion — don't expect incremental progress).
- Run id: `gh run list --branch <branch> --limit 1 --json databaseId,headSha` (a run is bound to its head SHA and immutable — re-inspect it instead of re-running). Right after a push the check runs may not exist yet: `gh pr checks --watch` then exits 1 with `no checks reported`; fetch the run id first and use `gh run watch`.

Exit status: with `--exit-status` the verdict *is* the process exit code — never pipe a watch through `tail` (the pipeline then reports the last command's status and swallows failures); drop the pipe or use `set -o pipefail`.

Every watch runs as a **background** process; read its output when the completion notification
arrives, and keep working (or end the turn) in the meantime. A push cancels in-flight runs for the
same ref (workflow concurrency) — if you need the CI evidence for a specific head, start the watch
for that head's run before pushing the next commit.

**The rule covers every wait, not only CI (2026-10-13 user directive).** Local background
commands, browser gates, builds — any long-running task — must obtain their result in real time:
the task's own completion notification is the mechanism. Never `sleep N` + polling; a one-off
instantaneous look at a running task's log answers "where is it now" but is not a waiting
mechanism, and a blocking wait is not one either (2026-10-15).

Inspecting a run while it is in flight (a snapshot, not a way to wait):

- Which step a run is on now: `gh run view <run-id> --json status,conclusion,jobs --jq '.jobs[] | select(.status != "completed") | {name, status}'` (one call; do not loop it as a substitute for `gh run watch`).
- Failed-step evidence without re-running: `gh run view <run-id> --log-failed` (whole step log); narrow it by the step's own log markers, e.g. `gh run view <run-id> --log 2>/dev/null | grep -a '\[armor-aiming\]'`.
- Which step costs what: `gh api repos/<owner>/<repo>/actions/runs/<run-id>/jobs --jq '.jobs[] | select(.name|test("<job>")) | .steps[] | "\(.number). \(.name) \((.completed_at|fromdate) - (.started_at|fromdate))s \(.conclusion)"'`.

## Parser and contract changes

- Preserve the repository's fail-closed behavior for ambiguous replay evidence; do not guess protocol semantics from a single sample.
- Public replay/facet contract changes must be documented together with the implementation.
- WotbTools consumes this repository as the upstream replay parser. Parser fixes belong here first, then WotbTools updates its pinned Agent release.

## Documentation follows the change

Documentation ships in the **same commit** as the change it describes — not as a follow-up, and
never skipped because the change "looks small":

- `docs/index.md` — progress/errata entry: what changed, the evidence it rests on, and the blast
  radius (which maps / artifacts / consumers are affected).
- `docs/data-inventory.md` + `docs/game-data-sources.md` — data-plane facts (pack stats and
  per-directory listings, generation and distribution chains, publishing pitfalls). They share one
  set of facts: change one and sync the other.
- Refresh the concrete numbers (counts, sizes, timestamps, hashes) whenever the artifact they
  describe is regenerated — a stale figure is worse than no figure.
- **Reverting a change means deleting its documentation**: no doc may keep describing behavior,
  artifacts or pitfalls that no longer exist.

## Frontend ownership (read before touching `frontend/`)

**This repository does not own the product frontend.** Frontend development **and frontend
testing** happen in the WotbTools repository (https://github.com/A158Coke/WotbTools).

- Any Web UI / 3D replay scene / playback control / label / effect / styling change — and any
  frontend test for it — belongs in WotbTools (https://github.com/A158Coke/WotbTools), not here:
  a frontend change made here is dead code, nothing deploys or maintains this copy. When a task
  reads as a UI problem (rendering, HUD, overlays, interaction), the implementation target is
  WotbTools; this repository covers parser / facet / data / asset-pipeline work.
- `frontend/` is frozen for local debugging (`cargo run --release -- web`) — leave it read-only.
  All frontend testing (including parser/pipeline playback verification) runs the WotbTools dev
  server; see [§All frontend testing runs the WotbTools frontend](#all-frontend-testing-runs-the-wotbtools-frontend-local-dev-build).
- Parser-side support lands here first, then the UI change in WotbTools (e.g. a data field the
  UI consumes).
- **Android version sync (`wotbVersion` in WotbTools `android/gradle.properties`)**: the APK
  embeds a snapshot of the shared web bundle, so any change altering what that bundle serves is
  an Android runtime change and needs a bump in the *same* WotbTools PR (its `.agents/AGENTS.md`
  "Android Version-as-Code" rule; `android-release.yml` only listens for that file on main).
  Scope: scene-runtime code (playbackScene, shaders, asset consumption), **WASM pin bumps**
  (`deploy/agent/source.json` — the release workflow fetches the pin into APK assets and records
  `agentWasm.commit/release` in `bundle-manifest.json`, so an unbumped pin ships a different
  runtime under the same versionCode), parser-facing contract adaptations, and packaged
  asset/serving-path changes. Server-only, asset-origin-only and docs changes skip it. A release here
  needs no bump by itself — its *consuming pin bump* in WotbTools does.
- **Asset-data changes must be published to the Alibaba asset origin** (`https://assets.wotbtools.com`,
  host `ali-sh-asset-01` / 47.101.141.111, `/srv/wotb-assets/releases/<pack-id>` + `current`).
  COS (`wotbtools-assets-1478073677`) is **legacy only** (old APKs and temporary rollback):
  never publish new content to the COS bucket root. Map re-export, ground rebake, texture/material fixes.
  Tank models are the same deal — the shipped bytes are our own export, so a tank fix is
  `python tools/export_tank_glb.py --tank <id> --out data/cache/models`（全量：先导
  `data/cache/local_models` 再同步两目录，二者应逐辆一致），随后把同批文件替换进
  `release/asset_pack/glb/<id>/`。Cover icons likewise are our own client export since
  2026-10-15: `python tools/export_tank_icons.py --all --out data/cache/local_tank_icons`
  then sync into `data/cache/tank_images/`（两目录逐字节一致，打包器读它）。Configurator
  data is asset data too: `python tools/export_research_loadout.py --out data/research`
  then `scripts/export_asset_pack.py` copies all non-icon files to `research/**`; global icons
  stay in WotbTools `common/research/icons/`. Publish a new asset release so new tanks/items
  work without a WotbTools release. Preview-client tanks absent from the live client use the
  incremental mode:
  `python tools/export_research_loadout.py --out data/research --game-data <preview Data root>
  --bridge data/preview_tank_id_bridge.json --modules-only --merge` (adds/replaces only those
  tank entries; per-tank `gameVersion` + `meta.preview` record provenance).
  Note `scripts/export_asset_pack.py` **overwrites overhead-baked grounds**, so after an export
  re-run the composite reading the pre-composite bases and the render cache:
  `python tools/composite_overhead.py --pack release/asset_pack --all --write --render-dir release/overhead-bake --base-dir data/cache/maps`
  (orientation is the fixed contract constant `YX`; `--verify` reads back what a pack actually
  baked — see [docs/game-data-sources.md](docs/game-data-sources.md) §5.5). **Archive the renders
  first**: the bake writes them into the *pack* (`<pack>/overhead/`), so run
  `python tools/archive_overhead_bake.py --clear-pack`（成对校验 + 世代体检，`--verify-only` 只体检）
  before anything clears that dir. **Precondition**: the
  render cache must be the same generation as the shipped grounds — its `meta` must carry
  `ground=true` (i.e. it contains the layered-ground pass); a cache without that flag predates
  the P2 bake — the composite now **refuses to write** such renders by default (fail-closed,
  `--allow-legacy-render` is diagnostics-only) and would otherwise silently roll all 36 grounds
  back a generation (re-bake upstream with `bake-ground-overhead.mjs --all` first; shipped bytes
  are recoverable read-only from the current asset origin — see §5.5 closing note). **The 2D tactical base
  maps are pack content too** (`release/asset_pack/map2d/` — the OTA channel the Android app
  validates against): the packer wipes the whole pack dir, so after every re-pack re-run
  `python tools/export_2d_basemaps.py` (staging `release/2d-basemaps/` + pack `map2d/` + its index)
  **before** re-stamping — [docs/game-data-sources.md](docs/game-data-sources.md) §5.6. Then
  **materialize per-variant map dirs** — `python tools/materialize_destructible_variants.py
  --pack release/asset_pack --map-index map_index.json`（多变体 space 的 lka 每变体一张；只给
  映射确有差异的变体建独立 `map/<key>/`，共享资产硬链接，index 按 map_id 指向本 key；必须在
  composite/2D 之后、manifest 重算之前）— then **re-stamp the manifest against the pack's actual
  bytes** — `python scripts/export_asset_pack.py --refresh-manifest`（合成器在打包之后改写
  `map/*/ground.webp`、变体工具新增目录；不重算则 manifest 与包内件不一致）.
  **Publish to Alibaba as a new immutable release** (never overwrite the live tree; never upload
  the whole pack to COS):
  1. `scp` only the changed files to `/srv/wotb-assets/incoming-<batch>/`（密钥
     `~/.ssh/wotb_asset_upload`，用户 `admin@47.101.141.111`；凭据不入库）。
  2. On the host: `cp -al releases/<current> releases/<new>`；需要新变体目录时
     `cp -al releases/<new>/map/<base> releases/<new>/map/<variant>`。
  3. Overlay changed files with `cp --remove-destination`（或临时文件 + `os.replace`）——
     **绝不能用普通 `cp`**：未变更文件与上一 release 共享 inode，截断一个就同时改坏回滚版本。
  4. Regenerate `manifest.json` on the host over the new release（排除 `manifest.json` /
     `version.json` / `delta.json`；保留 `version`/`data_version`/`upstream_commit`/`worktree_dirty`，刷新
     `generated` 与逐文件 sha256），写 `delta.json`，并复核 **0 mismatch**——包括上一 release
     的 manifest/index 哈希未变。
  5. `ln -s releases/<new> current.new && mv -T current.new current`（旧式单包切换；按游戏版本
     的 dataset 发布走第 6 步，不要手改 `datasets` 符号链接）；从
     `https://assets.wotbtools.com` 逐文件回拉比对 sha256，并在 WotbTools 仓跑
     `deploy/assets/verify.sh https://assets.wotbtools.com`。旧 release 保留回滚。
  6. **按游戏版本的 dataset**：该 release 属于某个 `dataset_id` 时，先
     `python tools/asset_release.py stamp --pack <release-dir> --dataset-id <id> --game-version <game_version>`
     （预览变体加 `--variant`），再在主机执行
     `bash deploy/assets/dataset.sh link --dataset <id> --release <new>`（自动重算
     `versions.json` 并 verify）；只有需要改变旧客户端默认入口时才 `dataset.sh default`。
     回滚用 `dataset.sh rollback`，不要手改 `current`/`datasets` 符号链接。
  7. **内容寻址对象库（推荐，跨版本去重）**：`/srv/wotb-assets/objects/<sha[:2]>/<sha256>`
     保存唯一内容，release 树改为对象硬链接（Caddy 静态伺服不变）。新发布可
     `asset_store.py import --manifest incoming/manifest.json --files incoming --objects objects`
     → `materialize --manifest ... --objects ... --out releases/<new> --metadata incoming`
     → `dataset.sh link`；沿用 `cp -al` 的旧流程则在新 release 上跑
     `asset_store.py adopt --releases releases/<new> --objects objects` 收敛。prune + 清理
     staging 后 `asset_store.py gc`（`--dry-run` 先看）。`objects` 与 `releases` 必须同盘；
     备份必须用 `rsync -aH`。实测单包内重复 4.9%，3 包合计 67.6%；完整契约见
     [docs/asset-store.md](docs/asset-store.md)。
  The manifest's **per-file sha256** is the verification authority — a content change with the same
  size must still ship (vertex bakes, 2026-10-10). `tools/upload_asset_pack_cos.py` is
  **legacy-only**（旧 APK/回滚）；其 `--only <prefix>` 语义不适用于新 origin。
- Background: [README §与 WotbTools 的关系](README.md), [frontend/AGENTS.md](frontend/AGENTS.md),
  [docs/architecture-debt.md](docs/architecture-debt.md).

## Visual verification belongs to the user

**Do not perform visual acceptance yourself.** For 3D replay playback, tank/model viewing,
scene rendering, overlays, effects and any other "does it look right" question:

- Do not launch or drive a browser (including the in-app browser) to screen-read, screenshot or
  otherwise eyeball the 3D output, and do not present screenshots as acceptance evidence.
- Instead land the change with tests that lock the invariant — pure-function unit tests,
  source-level wiring guards for the scene kernel, and the repository's automated browser gates
  — then state explicitly what the user should look at and hand the visual check over.
- This applies to both sides of the split: `frontend/` here is frozen, and WotbTools is where
  the UI lives; either way the visual verdict is the user's.

Rationale: the 3D scene depends on real GPU/terrain/asset-plane state that a detached,
throttled or headless browser does not faithfully reproduce; agent-driven screenshots cost many
round-trips and still leave the verdict unproven.

## All frontend testing runs the WotbTools frontend (local dev build)

**The test frontend is always the WotbTools frontend, never this repository's frozen
`frontend/`.** The frozen copy predates current scene features (e.g. destructible tree-fall) and
only exists for `cargo run --release -- web` self-hosting — verifying against it proves nothing.
Standard local test posture:

1. `node scripts/serve_asset_pack.mjs 8123` — serves `release/asset_pack/` with CORS. **Run it from
   this repo's root**: the script resolves the pack path relative to cwd
   (`resolve(process.argv[3] || 'release/asset_pack')`), so starting it anywhere else silently 404s
   every asset (the frontend just loses models/maps). If the port is already bound, an instance is
   likely still running: verify it serves the current pack
   (`curl http://127.0.0.1:8123/index.json`) instead of starting a second one — the script reads
   files from disk per request, so a running instance automatically picks up rebuilt packs.
   **数据版本选择**：第三个参数是 datasets 根目录（默认 `release/datasets`），脚本同时提供
   `/v/<id>/...` 与 `release/versions.json`；目录布局按 `datasets/<id> -> ../releases/<release-id>`，
   用 `python tools/asset_release.py stamp/catalog` 生成元数据。前端在
   `?assets=http://127.0.0.1:8123` 下用 `?data=<id>` 或侧边栏 logo 下方的版本下拉切换（移动端在
   「更多 → 显示设置」）。
2. In the WotbTools checkout, `frontend/.env.local` must contain
   `VITE_ASSET_BASE_URL=http://127.0.0.1:8123` (gitignored; `?assets=` in a URL overrides it and
   persists to localStorage, so clear it with an empty `?assets=` when returning to the default).
3. `npm run dev` in the WotbTools `frontend/` — Vite takes the first free port from 5173
   (stale instances often hold 5173/5174; use the port it actually prints). Without a local
   backend on 8087, `/api` probes fail — replay/3D testing is unaffected (those paths are local),
   and `npm run dev:production-remote` proxies `/api` to the production site when backend data
   is needed.
4. **Give the user a login-free test page** (2026-10-14 user directive). Personal-replay 3D/shots
   otherwise demand a production Keycloak login; apply the **local, never-committed** dev-gated
   login stub per the WotbTools manual (`docs/frontend/local-testing.md` §4.1) and open
   `http://localhost:<port>/?view=agent-replay&localuser=1&admin=1` (add the committed `admin=1`
   role-visibility bypass as usual), then hand the page over. The stub grants
   `authenticated=true` with role `wotbtools-user` only, provides **no credentials** (backend
   calls still 401 — local capabilities only) and must never enter a commit.
5. The WotbTools side of this workflow — pinned Agent WASM artifacts (`scripts/fetch-agent-wasm.sh`,
   must be re-run whenever `deploy/agent/source.json` changes), the local admin-view bypass,
   the login-free local test page (§4.1), self-check probes and troubleshooting — is documented
   in its `docs/frontend/local-testing.md` (plus `docs/frontend/local-production-dev.md` and
   `frontend/AGENTS.md`). **Follow that manual for every subsequent local test**: this list is
   the summary, the manual is the authority (login-free test page, commit discipline for local
   scaffolding, self-check probes, troubleshooting).

### Testing against the latest local WASM and asset pack

The WotbTools frontend consumes two build artifacts from this repository; keeping them current
with local changes is part of the test posture:

**WASM parser** — WotbTools pins a released artifact, not local working-tree code
(`deploy/agent/source.json`: repo/ref/release/asset/sha256; the fetch script verifies both
sha256 and fingerprint). The commit-addressed runtime URL (`/wasm/<ref>/…`) means one frontend
build only ever loads its own pin:

```bash
# In the WotbTools checkout — after this repo has published a release (or bumped the pin):
bash scripts/fetch-agent-wasm.sh        # downloads, sha256+fingerprint verifies, unpacks
```

**A pin bump is not just the fetch script.** WotbTools' AI-projection goldens
(`common/fixtures/ai-projection/*.json.gz`) embed the pin's provenance
(`projection.engine.agentRelease` / `agentCommit`), so a pin bump must regenerate them in the
final branch state —
`WOTB_UPDATE_AI_PROJECTION_GOLDEN=1 npx vitest run src/replay-local/ai/toClientAiReviewProjection.test.ts`
— and the regenerated goldens must be committed in the same PR, with `battle`/`projection`
verified field-for-field identical (only provenance changes). Skip it and
`toClientAiReviewProjection.test.ts` fails on 3 fixtures with a provenance mismatch that names
only release/commit — easy to misread as a WASM behavior regression; when in doubt, compare the
two releases' `wotb_replay_wasm_bg.wasm` sha256 (they can be byte-identical: a version-only
release changes no parser behavior).

To test **unreleased local parser changes**, either publish a release first
(bump `workspace.metadata.release.version` → release workflow) or build from source with
`scripts/build-agent-wasm.sh` (fallback path). Symptom of a stale pin after `source.json`
changes: `/wasm/<ref>/…` 404 → "engine load failed" in 3D / shots / armor while everything
else works — rerun the fetch script.

**Asset pack** — `release/asset_pack/` is gitignored and served by `serve_asset_pack.mjs` from
disk, so it is always "local latest" as long as it has been rebuilt. Full refresh chain when
map/tooling changes land (each step overwrites the previous one's output):

```bash
wotb-agent dump-shell-kinds --out data/shell_kinds.json           # shell-kind table (must come
                                                                  # from the same tanks.pb the
                                                                  # pack will ship; packer
                                                                  # fails closed without it)
python tools/export_tank_icons.py --all --out data/cache/local_tank_icons   # client cover icons
cp data/cache/local_tank_icons/*.webp data/cache/tank_images/     # sync (the two dirs must stay
                                                                  # byte-identical; packer reads
                                                                  # cache/tank_images)
python tools/export_tank_icons.py --emit-crops   # per-tank icon content boxes -> pack
                                                 # tank_images/crops.json (display-layer crop;
                                                 # icons re-exported ⇒ must re-run; packer
                                                 # fails closed if missing or count-mismatched)
python tools/export_research_loadout.py --out data/research  # configurator data (packer copies
                                                 # all non-icon files to research/**;
                                                 #  icons stay in WotbTools common)
python scripts/export_asset_pack.py --map-index map_index.json   # rebuild pack (overwrites
                                                                 # overhead-baked grounds!)
python tools/composite_overhead.py --pack release/asset_pack --all --write \
    --render-dir release/overhead-bake --base-dir data/cache/maps  # re-bake overhead views
                                                                   # (orientation = fixed YX;
                                                                   #  --base-dir avoids double-baking
                                                                   #  on the already-composited files)
python tools/export_2d_basemaps.py                                # pack map2d/ + index (after re-pack)
python tools/materialize_destructible_variants.py --pack release/asset_pack \
    --map-index map_index.json                                    # per-variant map dirs (hardlinks)
python scripts/export_asset_pack.py --out release/asset_pack --refresh-manifest
# publish a new Alibaba release if production users must see it (see §Asset-data changes above);
# COS is legacy only — do not upload new content to the COS bucket root
```

`export_asset_pack.py` rebuilds `ground.webp` from the client colormap and **destroys the
overhead-baked grounds** — never serve a freshly exported pack without re-running the composite.
`deploy/agent/source.json` pin and WASM artifacts (`common/assets/wasm/`) are gitignored in
WotbTools; the pin file itself is committed.

