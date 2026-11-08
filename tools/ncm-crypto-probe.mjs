#!/usr/bin/env node
// 网易云 weapi / eapi 加解密探针 + 黄金向量生成器
//
//   1) 只用于验证协议实现，不涉及登录态与付费内容；
//   2) `--vectors` 输出确定性黄金向量，供 Rust 侧 #[test] 断言；
//   3) 默认模式实测线上接口，验收标准：weapi/eapi 搜索都返回 code=200，
//      播放直链返回 expi=1200 且支持 HTTP Range(206)。
//
// 运行：
//   node tools/ncm-crypto-probe.mjs            # 线上实测
//   node tools/ncm-crypto-probe.mjs --vectors  # 黄金向量
import crypto from 'node:crypto';
import { pathToFileURL } from 'node:url';

const IV = Buffer.from('0102030405060708');
const PRESET_KEY = Buffer.from('0CoJUm6Qyw8W8jud');
const EAPI_KEY = Buffer.from('e82ckenh8dichen8');
const MODULUS = BigInt(
  '0x' +
    '00e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a87' +
    '6aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c' +
    '9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d546' +
    'b8e289dc6935b3ece0462db0a22b8e7'
);
const PUB_EXP = 0x010001n;

const b64 = (buf) => buf.toString('base64');
const md5hex = (s) => crypto.createHash('md5').update(s, 'utf8').digest('hex');

function aesCbc(buf, key) {
  const c = crypto.createCipheriv('aes-128-cbc', key, IV);
  return Buffer.concat([c.update(buf), c.final()]);
}

function aesEcb(buf, key) {
  const c = crypto.createCipheriv('aes-128-ecb', key, null);
  return Buffer.concat([c.update(buf), c.final()]);
}

function modPow(base, exp, mod) {
  let r = 1n;
  base %= mod;
  while (exp > 0n) {
    if (exp & 1n) r = (r * base) % mod;
    base = (base * base) % mod;
    exp >>= 1n;
  }
  return r;
}

/** weapi：两层 AES-128-CBC + 对随机密钥做 RSA(no padding) 加密 */
export function weapi(data, { nonce } = {}) {
  const text = JSON.stringify({ ...data, csrf_token: '' });
  const key = nonce ?? crypto.randomBytes(8).toString('hex').slice(0, 16);
  const enc1 = b64(aesCbc(Buffer.from(text, 'utf8'), PRESET_KEY));
  const enc2 = b64(aesCbc(Buffer.from(enc1, 'utf8'), Buffer.from(key)));
  // 关键细节：RSA 前把 16 字节密钥逆序，再按大端转成整数
  const reversed = key.split('').reverse().join('');
  const encSecKey = modPow(
    BigInt('0x' + Buffer.from(reversed, 'utf8').toString('hex')),
    PUB_EXP,
    MODULUS
  )
    .toString(16)
    .padStart(256, '0');
  return { params: enc2, encSecKey };
}

/** eapi：AES-128-ECB，密钥固定，payload 内嵌一段防篡改摘要 */
export function eapi(path, data) {
  const text = JSON.stringify(data);
  const digest = md5hex(`nobody${path}use${text}md5forencrypt`);
  const plain = `${path}-36cd479b6b5-${text}-36cd479b6b5-${digest}`;
  return aesEcb(Buffer.from(plain, 'utf8'), EAPI_KEY).toString('hex').toUpperCase();
}

const UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ' +
  'Chrome/120.0.0.0 Safari/537.36';

const HEADERS = {
  'User-Agent': UA,
  Referer: 'https://music.163.com',
  Origin: 'https://music.163.com',
  'Content-Type': 'application/x-www-form-urlencoded',
  Cookie: 'os=pc; appver=2.10.12; deviceId=probe0000000000; channel=netease;',
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function postJson(path, body, headers = HEADERS) {
  const res = await fetch(`https://music.163.com${path}`, {
    method: 'POST',
    headers,
    body: new URLSearchParams(body),
  });
  const text = await res.text();
  try {
    return { status: res.status, json: JSON.parse(text) };
  } catch {
    return { status: res.status, json: null, raw: text.slice(0, 160) };
  }
}

const weapiPost = async (path, data) => postJson(path, weapi(data));

// ---------------------------------------------------------------------------
// 黄金向量：给 Rust 侧 #[test] 用。输入固定、输出确定，任何实现漂移都会失败。
// ---------------------------------------------------------------------------
const VECTORS = {
  weapi: (() => {
    const input = { s: '晴天', type: 1, limit: 1, offset: 0 };
    const r = weapi(input, { nonce: '0123456789abcdef' });
    return { input, nonce: '0123456789abcdef', params: r.params, encSecKey: r.encSecKey };
  })(),
  eapi: (() => {
    const path = '/api/search/get';
    const input = { s: '晴天', type: 1, limit: 1, offset: 0 };
    return { path, input, params: eapi(path, input) };
  })(),
};

async function live() {
  const out = [];

  const s = await weapiPost('/weapi/search/get', { s: '晴天', type: 1, limit: 2, offset: 0 });
  out.push({
    api: 'weapi /weapi/search/get',
    http: s.status,
    code: s.json?.code,
    sample: (s.json?.result?.songs ?? []).map((x) => x.name),
  });

  const e = await postJson('/eapi/search/get', {
    params: eapi('/api/search/get', { s: '晴天', type: 1, limit: 2, offset: 0 }),
  });
  out.push({ api: 'eapi /eapi/search/get', http: e.status, code: e.json?.code });

  // 播放直链：注意上层 code 恒为 200，真实结果在 data[0].code
  const SONG = 26259733;
  let detail = null;
  for (let attempt = 1; attempt <= 3 && !detail; attempt++) {
    const r = await weapiPost('/weapi/song/enhance/player/url/v1', {
      ids: `[${SONG}]`,
      level: 'standard',
      encodeType: 'mp3',
    });
    detail = r.json?.data?.[0] ?? null;
    if (!detail?.url) await sleep(1200 * attempt);
  }
  const url = detail?.url?.replace(/^http:/, 'https:');
  out.push({
    api: 'weapi /weapi/song/enhance/player/url/v1',
    outerCode: 200,
    innerCode: detail?.code,
    expiSeconds: detail?.expi,
    br: detail?.br,
    size: detail?.size,
    md5: detail?.md5,
    gain: detail?.gain,
    peak: detail?.peak,
    urlPathShape: url ? url.split('/').slice(0, 4).join('/') + '/{ts}/{hash}/{id}.mp3' : null,
  });

  if (url) {
    const head = await fetch(url, { method: 'HEAD', headers: { 'User-Agent': UA } });
    const ranged = await fetch(url, {
      headers: { 'User-Agent': UA, Range: 'bytes=1024-2047' },
    });
    out.push({
      api: 'audio url',
      headStatus: head.status,
      acceptRangesHeader: head.headers.get('accept-ranges'),
      contentLength: head.headers.get('content-length'),
      rangeStatus: ranged.status,
      contentRange: ranged.headers.get('content-range'),
      rangeBytes: (await ranged.arrayBuffer()).byteLength,
    });
  }

  // 歌词：yrc 并非每首都有，必须能回落到 lrc
  const lyric = await weapiPost('/weapi/song/lyric', {
    id: 1974443814,
    lv: -1,
    kv: -1,
    tv: -1,
    rv: -1,
    yv: -1,
  });
  const yrcLines = (lyric.json?.yrc?.lyric ?? '').split('\n').filter(Boolean);
  out.push({
    api: 'weapi /weapi/song/lyric',
    code: lyric.json?.code,
    keys: Object.keys(lyric.json ?? {}),
    yrcLines: yrcLines.length,
    yrcSample: yrcLines.find((l) => (l.match(/\(/g) ?? []).length >= 3) ?? null,
  });

  console.log(JSON.stringify(out, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.includes('--vectors')) {
    console.log(JSON.stringify(VECTORS, null, 2));
  } else {
    await live();
  }
}
