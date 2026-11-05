// 本机资产面静态服务：把 `release/asset_pack`（export_asset_pack.py 的产物）按消费方
// assetProvider 期望的布局伺服，并带上 CORS 头——WotbTools dev server（5173）跨源取
// GLB / 地图 / 坦克数据时浏览器才放行。
//
// 同时支持按游戏版本的数据集（dataset 版本管理）：
//   /v/<dataset_id>/...   → <datasetsDir>/<dataset_id>/...
//   /versions.json        → <datasetsDir>/../versions.json（asset_release.py catalog 的产物）
// 未提供 datasetsDir 时这些路径 404，根路径行为不变。
//
// 为什么需要：WotbTools 前端是 client-only + remote 资产面设计，dev 下没有构建期
// `VITE_ASSET_BASE_URL` 时 `assetProvider` 会显式报「资产源未配置」，地图地形 /
// 车模 GLB / 坦克 JSON 一律不加载（回放解析本身不依赖资产，所以现象是"只有模型没了"）。
// 标准本机联调姿势（两侧仓库都记在文档里）：
//   1) node scripts/serve_asset_pack.mjs 8123            # 本仓，伺服 release/asset_pack
//   2) frontend/.env.local: VITE_ASSET_BASE_URL=http://127.0.0.1:8123   # WotbTools 仓
//   3) npm run dev → http://localhost:5173/?view=agent-replay&agentViews=1
//
// 用法：node scripts/serve_asset_pack.mjs [port] [packDir] [datasetsDir]
//   默认：node scripts/serve_asset_pack.mjs 8123 release/asset_pack release/datasets
import { createReadStream, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join, normalize, resolve, sep } from 'node:path'

const port = Number(process.argv[2] || 8123)
const root = resolve(process.argv[3] || 'release/asset_pack')
const datasetsRoot = resolve(process.argv[4] || 'release/datasets')
const catalogPath = resolve(datasetsRoot, '..', 'versions.json')

const TYPES = {
  '.json': 'application/json', '.glb': 'model/gltf-binary', '.webp': 'image/webp',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.bin': 'application/octet-stream',
  '.u16.bin': 'application/octet-stream', '.wotbreplay': 'application/octet-stream',
}

function isInside(base, file) {
  return file === base || file.startsWith(base + sep)
}

/** 请求路径 → 磁盘文件；未知 dataset / 目录穿越一律 null。 */
function resolveUrl(urlPath) {
  if (urlPath === '/versions.json') {
    return existsSync(catalogPath) && statSync(catalogPath).isFile() ? catalogPath : null
  }
  const versioned = urlPath.match(/^\/v\/([^/]+)(\/.*)?$/)
  if (versioned) {
    const [, datasetId, rest = '/'] = versioned
    if (!/^[a-z0-9][a-z0-9.-]*$/.test(datasetId)) return null
    const base = join(datasetsRoot, datasetId)
    const file = join(base, normalize(rest).replace(/^([/\\])+/, ''))
    return isInside(base, file) ? file : null
  }
  const file = join(root, normalize(urlPath).replace(/^([/\\])+/, ''))
  return isInside(root, file) ? file : null
}

createServer((req, res) => {
  const urlPath = decodeURIComponent((req.url || '/').split('?')[0])
  const file = resolveUrl(urlPath)
  if (!file || !existsSync(file) || !statSync(file).isFile()) {
    res.writeHead(404, { 'Access-Control-Allow-Origin': '*' })
    res.end('not found: ' + urlPath)
    return
  }
  const size = statSync(file).size
  const ext = extname(file).toLowerCase()
  res.writeHead(200, {
    'Content-Type': TYPES[ext] || 'application/octet-stream',
    'Content-Length': size,
    'Access-Control-Allow-Origin': '*',
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'no-cache',
  })
  if (req.method === 'HEAD') { res.end(); return }
  createReadStream(file).pipe(res)
}).listen(port, '127.0.0.1', () => {
  console.log(`asset pack: http://127.0.0.1:${port}/  (root=${root})`)
  console.log(`datasets:   http://127.0.0.1:${port}/v/<id>/  (datasetsDir=${datasetsRoot})`)
})