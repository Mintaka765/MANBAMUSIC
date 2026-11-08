// 排查：为什么有些歌在官方 App 免费，在这里变成"试听/不可播"。
// 对比两件事：① 匿名会话注册前后，搜索结果里的 privilege 与直链差异；
// ② 不同 level 下服务端返回的 code / br / freeTrialInfo。
import { weapi } from './ncm-crypto-probe.mjs';

const UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ' +
  'Chrome/120.0.0.0 Safari/537.36';

function headers(cookie) {
  return {
    'User-Agent': UA,
    Referer: 'https://music.163.com',
    'Content-Type': 'application/x-www-form-urlencoded',
    ...(cookie ? { Cookie: cookie } : {}),
  };
}

async function post(path, data, cookie) {
  const { params, encSecKey } = weapi(data);
  const res = await fetch(`https://music.163.com${path}`, {
    method: 'POST',
    headers: headers(cookie),
    body: new URLSearchParams({ params, encSecKey }),
  });
  return { res, json: await res.json().catch(() => null) };
}

// 1) 注册匿名会话，收集 Set-Cookie
const anon = await post('/weapi/register/anonimous', { username: '' });
const setCookie = anon.res.headers.getSetCookie?.() ?? [];
const cookieFromAnon = setCookie.map((c) => c.split(';')[0]).join('; ');
console.log('anonimous code =', anon.json?.code, '| cookies =', setCookie.length);
console.log('cookie names =', setCookie.map((c) => c.split('=')[0]).join(', '));

const baseCookie =
  'os=pc; appver=2.10.12; deviceId=probe0000000000; channel=netease;';
const withAnon = `${baseCookie}; ${cookieFromAnon}`;

// 2) 搜索，比较匿名前后 privilege
async function searchAndInspect(label, cookie) {
  const { json } = await post(
    '/weapi/search/get',
    { s: '晴天', type: 1, limit: 6, offset: 0 },
    cookie,
  );
  const songs = json?.result?.songs ?? [];
  console.log(`\n== ${label} ==`);
  for (const song of songs) {
    const p = song.privilege ?? {};
    console.log(
      `id=${song.id} fee=${song.fee} st=${p.st} pl=${p.pl} dl=${p.dl} maxBr=${p.maxBr} :: ${song.name.slice(0, 18)}`,
    );
  }
  return songs;
}

const withoutSongs = await searchAndInspect('无匿名会话', baseCookie);
const withSongs = await searchAndInspect('有匿名会话', withAnon);

// 3) 对同一首歌，比较各 level 的直链
async function urlFor(id, level, cookie) {
  const { json } = await post(
    '/weapi/song/enhance/player/url/v1',
    { ids: `[${id}]`, level, encodeType: 'mp3' },
    cookie,
  );
  const d = json?.data?.[0] ?? {};
  return {
    level,
    innerCode: d.code,
    br: d.br,
    fee: d.fee,
    trial: d.freeTrialInfo
      ? `${Math.round((d.freeTrialInfo.end - d.freeTrialInfo.start) / 1000)}s`
      : null,
    hasUrl: !!d.url,
  };
}

const sample = withSongs.find((s) => s.fee === 8) ?? withSongs[0];
if (sample) {
  console.log(`\n== 直链对比（id=${sample.id} fee=${sample.fee}）==`);
  for (const cookie of [baseCookie, withAnon]) {
    const tag = cookie === baseCookie ? '匿名前' : '匿名后';
    for (const level of ['standard', 'exhigh', 'lossless']) {
      console.log(tag, await urlFor(sample.id, level, cookie));
    }
  }
}
