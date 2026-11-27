// How long Chromium's main thread works on a long list: first frame, one label
// changing, and a scroll step. The row is a label, a button and a check box.
//
//   node perf.mjs --out ../out/web
//
// Main-thread time (script, style, layout and recording the paint) is read from
// the DevTools protocol, so waiting for the next frame is not counted. Raster
// runs on other threads and is not included, which flatters the web a little
// against the other two, which paint on the calling thread.
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const outAt = args.indexOf('--out');
const out = outAt >= 0 ? args[outAt + 1] : join(here, '..', 'out', 'web');
const ROWS = (process.env.PARITY_ROWS ?? '100,1000,5000').split(',').map(Number);
const ROUNDS = 20;
mkdirSync(out, { recursive: true });

const browser = await chromium.launch();
const result = { framework: 'web', variants: { 'plain DOM': {} } };

for (const n of ROWS) {
  const page = await browser.newPage({ viewport: { width: 420, height: 460 } });
  await page.setContent('<!doctype html><body style="margin:0;font:14px system-ui"></body>');
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Performance.enable');
  const busy = async () => (await cdp.send('Performance.getMetrics')).metrics.find((m) => m.name === 'TaskDuration').value * 1000;
  const frames = () => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

  const html = `<div id="list" style="height:400px;width:420px;overflow:auto">${Array.from({ length: n }, (_, i) =>
    `<div class="row" style="display:flex;gap:8px;align-items:center;height:36px"><span>Item ${i}: some text</span><button>Open</button><label><input type="checkbox">Done</label></div>`).join('')}</div>`;

  let before = await busy();
  await page.evaluate((html) => { document.body.innerHTML = html; }, html);
  await frames();
  const first = (await busy()) - before;

  before = await busy();
  for (let k = 0; k < ROUNDS; k++) {
    await page.evaluate((k) => { document.querySelector('#list .row span').textContent = `Item 0: changed ${k}`; }, k);
    await frames();
  }
  const update = ((await busy()) - before) / ROUNDS;

  before = await busy();
  for (let k = 1; k <= ROUNDS; k++) {
    await page.evaluate((k) => { document.getElementById('list').scrollTop = k * 120; }, k);
    await frames();
  }
  const scroll = ((await busy()) - before) / ROUNDS;

  result.variants['plain DOM'][n] = { first, update, scroll };
  console.log(`web: ${n} rows  first ${first.toFixed(1)} ms  update ${update.toFixed(2)} ms  scroll ${scroll.toFixed(2)} ms`);
  await page.close();
}
await browser.close();
writeFileSync(join(out, 'perf.json'), JSON.stringify(result, null, 2));
