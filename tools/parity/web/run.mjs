// The web side of the parity bench: renders each scenario as plain HTML and
// drives it in Chromium with real mouse and keyboard input. Chromium is the
// engine behind Tauri's WebView2 on Windows, so what it does is what a Tauri
// front end does.
//
//   node run.mjs --out ../out/web [scenario ids ...]
import { createRequire } from 'node:module';
import { readFileSync, readdirSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const outAt = args.indexOf('--out');
const out = outAt >= 0 ? args[outAt + 1] : join(here, '..', 'out', 'web');
const only = args.filter((a, i) => !a.startsWith('--') && i !== outAt + 1);
mkdirSync(out, { recursive: true });

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const px = (v) => (typeof v === 'number' ? `${v}px` : v);

// The spec's `align` defaults to "start": children keep their own width.
const MAP = { start: 'flex-start', center: 'center', end: 'flex-end', stretch: 'stretch', 'space-between': 'space-between', 'space-around': 'space-around', 'space-evenly': 'space-evenly' };

// What a framework cannot express is listed in `unsupported`; the web can do all of it.
function css(node, extra = '') {
  // An omitted `shrink` is the web's own default of 1; the others default to 0.
  const s = [`flex:${node.grow ?? 0} ${node.shrink ?? 1} ${node.basis != null ? px(node.basis) : 'auto'}`, extra];
  if (node.width != null) s.push(`width:${px(node.width)}`);
  if (node.height != null) s.push(`height:${px(node.height)}`);
  if (node.widthPercent != null) s.push(`width:${node.widthPercent * 100}%`);
  if (node.heightPercent != null) s.push(`height:${node.heightPercent * 100}%`);
  if (node.minWidth != null) s.push(`min-width:${px(node.minWidth)}`);
  if (node.maxWidth != null) s.push(`max-width:${px(node.maxWidth)}`);
  if (node.minHeight != null) s.push(`min-height:${px(node.minHeight)}`);
  if (node.maxHeight != null) s.push(`max-height:${px(node.maxHeight)}`);
  if (node.aspectRatio != null) s.push(`aspect-ratio:${node.aspectRatio}`);
  if (node.alignSelf) s.push(`align-self:${MAP[node.alignSelf]}`);
  if (node.margin != null) s.push(`margin:${px(node.margin)}`);
  for (const side of node.marginAuto || []) s.push(`margin-${side}:auto`);
  if (Array.isArray(node.border)) s.push(`border-style:solid;border-color:#8aa4c8;border-width:${node.border.map(px).join(' ')}`);
  else if (node.border != null) s.push(`border:${px(node.border)} solid #8aa4c8`);
  if (node.overflow) s.push(`overflow:${node.overflow}`);
  if (node.background) s.push(`background:${node.background}`);
  if (node.visible === false) s.push('display:none');
  return s.filter(Boolean).join(';');
}

function render(node) {
  const id = esc(node.id ?? '');
  const dis = node.disabled ? ' disabled' : '';
  switch (node.type) {
    case 'column':
    case 'row': {
      const dir = (node.type === 'column' ? 'column' : 'row') + (node.reverse ? '-reverse' : '');
      const gap = node.gap != null ? `gap:${px(node.gap)}` : '';
      const gaps = `${node.rowGap != null ? `row-gap:${px(node.rowGap)};` : ''}${node.columnGap != null ? `column-gap:${px(node.columnGap)};` : ''}`;
      const pad = node.padding != null ? `padding:${[].concat(node.padding).map(px).join(' ')}` : '';
      const content = node.alignContent ? `align-content:${MAP[node.alignContent]};` : '';
      const flex = `display:flex;${node.wrap ? 'flex-wrap:wrap;' : ''}flex-direction:${dir};align-items:${MAP[node.align ?? 'start']};justify-content:${MAP[node.justify ?? 'start']};${gap};${gaps}${pad};${content}`;
      // A plain element cannot disable what is inside it; a fieldset can.
      if (node.disabled) return `<fieldset data-id="${id}" disabled style="${css(node, `padding:0;${flex};border:0;margin:0;min-width:0`)}">${(node.children || []).map(render).join('')}</fieldset>`;
      return `<div data-id="${id}" style="${css(node, flex)}">${(node.children || []).map(render).join('')}</div>`;
    }
    case 'grid': {
      // Track sizes are numbers (pixels) or `fr`, `auto`, as the stylesheet reads them.
      const tracks = (t) => String(t).split(/\s+/).map((x) => (/^[0-9.]+$/.test(x) ? `${x}px` : x)).join(' ');
      const gap = node.gap != null ? `gap:${px(node.gap)};` : '';
      const cols = node.columns ? `grid-template-columns:${tracks(node.columns)};` : '';
      const rows = node.rows ? `grid-template-rows:${tracks(node.rows)};` : '';
      const pad = node.padding != null ? `padding:${px(node.padding)};` : '';
      const kids = (node.children || []).map((c) => {
        const place = (c.column ? `grid-column:${c.column} / span ${c.colSpan ?? 1};` : '') + (c.row ? `grid-row:${c.row} / span ${c.rowSpan ?? 1};` : '');
        return `<div style="${place}min-width:0;display:flex;flex-direction:column;align-items:stretch">${render({ ...c, grow: 0, shrink: 1 })}</div>`;
      }).join('');
      return `<div data-id="${id}" style="${css(node, `display:grid;${cols}${rows}${gap}${pad}align-items:start;justify-items:start`)}">${kids}</div>`;
    }
    case 'stack': {
      const kids = (node.children || []).map((c) => {
        const at = c.position ?? { x: 0, y: 0 };
        const z = c.z != null ? `;z-index:${c.z}` : '';
        // Wrapped so the child keeps its own style; the wrapper only places it.
        // `positionEnd` pins to the right and bottom instead of the left and top.
        const place = c.positionEnd ? `right:${c.positionEnd.right}px;bottom:${c.positionEnd.bottom}px` : `left:${at.x}px;top:${at.y}px`;
        return `<div style="position:absolute;${place}${z}">${render({ ...c, grow: 0, shrink: 0 })}</div>`;
      }).join('');
      return `<div data-id="${id}" style="${css(node, 'position:relative')}">${kids}</div>`;
    }
    case 'scroll':
      return `<div data-id="${id}" style="${css(node, 'display:flex;flex-direction:column;align-items:flex-start;overflow:auto')}">${(node.children || []).map(render).join('')}</div>`;
    case 'panel':
      return `<div data-id="${id}" style="${css(node)}"></div>`;
    case 'spacer':
      return `<div data-id="${id}" style="${css(node)}"></div>`;
    case 'label':
      return `<span data-id="${id}" style="${css(node)}">${esc(node.text)}</span>`;
    case 'button':
      return `<button data-id="${id}"${dis} style="${css(node)}">${esc(node.text)}</button>`;
    case 'checkbox':
    case 'switch': {
      const role = node.type === 'switch' ? ' role="switch"' : '';
      return `<label style="${css(node, 'display:flex;align-items:center;gap:6px')}"><input type="checkbox" data-id="${id}"${role}${node.checked ? ' checked' : ''}${dis}>${esc(node.text)}</label>`;
    }
    case 'radiogroup':
      return `<div role="radiogroup" data-id="${id}" style="${css(node, 'display:flex;flex-direction:column;align-items:flex-start')}">${node.options
        .map((o, i) => `<label style="display:flex;align-items:center;gap:6px;flex:none"><input type="radio" name="${id}" value="${i}" data-id="${esc(o.id)}"${node.value === i ? ' checked' : ''}>${esc(o.text)}</label>`)
        .join('')}</div>`;
    case 'input':
      return `<input type="text" data-id="${id}" aria-label="${esc(node.label ?? '')}" value="${esc(node.value ?? '')}"${dis} style="${css(node)}">`;
    case 'slider':
      return `<input type="range" data-id="${id}" aria-label="${esc(node.label ?? '')}" min="${node.min}" max="${node.max}" step="${node.step}" value="${node.value}"${dis} style="${css(node)}">`;
    default:
      throw new Error(`unknown widget type ${node.type}`);
  }
}

const shell = (spec) => `<!doctype html><html><head><meta charset="utf-8"><style>
  :root { color-scheme: dark; }
  *, *::before, *::after { box-sizing: border-box; }
  html, body { margin: 0; background: #1b2230; color: #e6ebf5; font: 14px/1.2 system-ui, sans-serif; }
  #root { width: ${spec.size[0]}px; height: ${spec.size[1]}px; overflow: hidden; display: flex; flex-direction: column; }
</style></head><body><div id="root">${render({ ...spec.ui, width: spec.size[0], height: spec.size[1] })}</div>
<script>
  window.__log = [];
  const root = document.getElementById('root');
  root.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-id]');
    if (b && !b.disabled) __log.push('click:' + b.dataset.id);
  });
  root.addEventListener('change', (e) => {
    const t = e.target;
    if (t.type === 'checkbox') __log.push('toggle:' + t.dataset.id + ':' + t.checked);
    if (t.type === 'radio') __log.push('select:' + t.name + ':' + t.value);
  });
  root.addEventListener('input', (e) => {
    const t = e.target;
    if (t.type === 'text') __log.push('input:' + t.dataset.id + ':' + t.value);
    if (t.type === 'range') __log.push('slide:' + t.dataset.id + ':' + Number(t.value));
  });
</script></body></html>`;

const ROLES = { StaticText: 'text', InlineTextBox: 'text', LabelText: 'text', generic: 'generic', none: 'generic', radiogroup: 'group', group: 'group' };

async function run(spec, browser) {
  const page = await browser.newPage({ viewport: { width: spec.size[0], height: spec.size[1] } });
  await page.setContent(shell(spec));
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');

  const ids = await page.evaluate(() => [...document.querySelectorAll('[data-id]')].map((e) => e.dataset.id));

  // The box a person aims at: a checkbox's label is the whole control.
  const rectOf = (id) =>
    page.evaluate((id) => {
      const el = document.querySelector(`[data-id="${id}"]`);
      if (!el.getClientRects().length) return [0, 0, 0, 0];
      const box = (el.type === 'checkbox' || el.type === 'radio' ? el.closest('label') : el).getBoundingClientRect();
      return [box.x, box.y, box.width, box.height];
    }, id);

  const centre = async (id, fx = 0.5, fy = 0.5) => {
    const [x, y, w, h] = await rectOf(id);
    return [x + w * fx, y + h * fy];
  };

  async function a11y(id) {
    const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
    const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: `[data-id="${id}"]` });
    const { nodes } = await cdp.send('Accessibility.getPartialAXTree', { nodeId, fetchRelatives: false });
    const n = nodes[0];
    if (!n || n.ignored) return { role: 'none', name: '' };
    const prop = (name) => n.properties?.find((p) => p.name === name)?.value?.value;
    const raw = n.role?.value ?? 'generic';
    const out = { role: ROLES[raw] ?? raw, name: n.name?.value ?? '' };
    if (prop('disabled')) out.disabled = true;
    const checked = prop('checked');
    if (checked !== undefined) out.checked = checked === 'true' ? true : checked === 'false' ? false : checked;
    if (n.value?.value !== undefined && n.value.value !== '') out.value = String(n.value.value);
    return out;
  }

  const observe = async (label) => {
    const state = await page.evaluate(() => {
      const a = document.activeElement;
      const owner = a && a !== document.body ? a.closest('[data-id]') : null;
      const values = {};
      for (const el of document.querySelectorAll('[data-id]')) {
        if (el.type === 'checkbox') values[el.dataset.id] = el.checked;
        else if (el.type === 'radio') { if (el.checked) values[el.name] = Number(el.value); }
        else if (el.type === 'text') values[el.dataset.id] = el.value;
        else if (el.type === 'range') values[el.dataset.id] = Number(el.value);
      }
      const scroll = {};
      for (const el of document.querySelectorAll('[data-id]')) {
        if (el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY === 'auto') scroll[el.dataset.id] = Math.round(el.scrollTop);
        else if (getComputedStyle(el).overflowY === 'auto') scroll[el.dataset.id] = 0;
      }
      return {
        focus: owner ? owner.dataset.id : null,
        focusVisible: owner ? a.matches(':focus-visible') : null,
        values, scroll, log: [...window.__log],
      };
    });
    const rects = {};
    const names = {};
    for (const id of ids) {
      rects[id] = (await rectOf(id)).map((v) => Math.round(v * 10) / 10);
      names[id] = await a11y(id);
    }
    return { type: 'observe', label, ...state, a11y: names, rects };
  };

  const records = [];
  const shots = [];
  const shot = async (label) => {
    const file = `${spec.id}--${records.length}.png`;
    await page.screenshot({ path: join(out, file), clip: { x: 0, y: 0, width: spec.size[0], height: spec.size[1] } });
    shots.push(file);
  };

  const key = (k) => (k === 'Space' ? ' ' : k);
  for (const step of spec.steps) {
    switch (step.do) {
      case 'observe': {
        records.push(await observe(step.label));
        await shot(step.label);
        break;
      }
      case 'click': {
        const [x, y] = await centre(step.target);
        await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.up();
        break;
      }
      case 'clickAt': {
        const [x, y] = await centre(step.target, step.fx, step.fy);
        await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.up();
        break;
      }
      case 'hover': {
        const [x, y] = await centre(step.target);
        await page.mouse.move(x, y);
        break;
      }
      case 'dragOff': {
        const [x, y] = await centre(step.target);
        await page.mouse.move(x, y); await page.mouse.down();
        await page.mouse.move(x + 150, y, { steps: 5 }); await page.mouse.up();
        break;
      }
      case 'wheel': {
        const [x, y] = await centre(step.target);
        await page.mouse.move(x, y); await page.mouse.wheel(0, step.dy);
        await page.waitForTimeout(150);
        break;
      }
      case 'press': await page.keyboard.press(key(step.key)); break;
      case 'type': await page.keyboard.type(step.text); break;
      case 'cursor': {
        const cursor = {};
        for (const id of step.targets) {
          const [x, y] = await centre(id);
          await page.mouse.move(x, y);
          cursor[id] = await page.evaluate(([x, y]) => {
            const el = document.elementFromPoint(x, y);
            if (!el) return 'offscreen';
            const c = getComputedStyle(el).cursor;
            // `auto` is the browser's choice: an I-beam over text, else the arrow.
            if (c !== 'auto') return c;
            return el.childNodes.length && [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim()) ? 'text' : 'default';
          }, [x, y]);
        }
        await page.mouse.move(0, 0);
        records.push({ type: 'cursor', label: step.label, cursor });
        break;
      }
      case 'tabSequence': {
        const focus = [];
        for (let i = 0; i < step.count; i++) {
          await page.keyboard.press(step.shift ? 'Shift+Tab' : 'Tab');
          focus.push(await page.evaluate(() => {
            const a = document.activeElement;
            return a && a !== document.body ? a.closest('[data-id]')?.dataset.id ?? null : null;
          }));
        }
        records.push({ type: 'tabs', label: step.label, focus });
        break;
      }
      default: throw new Error(`unknown step ${step.do}`);
    }
    await page.waitForTimeout(30);
  }
  await page.close();
  return { scenario: spec.id, framework: 'web', title: spec.title, size: spec.size, records, screenshots: shots, unsupported: [] };
}

const specs = readdirSync(join(here, '..', 'scenarios'))
  .filter((f) => f.endsWith('.json'))
  .map((f) => JSON.parse(readFileSync(join(here, '..', 'scenarios', f), 'utf8')))
  .filter((s) => !only.length || only.includes(s.id));

const browser = await chromium.launch();
for (const spec of specs) {
  const result = await run(spec, browser);
  writeFileSync(join(out, `${spec.id}.json`), JSON.stringify(result, null, 2));
  console.log(`web: ${spec.id} (${result.records.length} records)`);
}
await browser.close();
