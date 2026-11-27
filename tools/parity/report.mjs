// Builds one self-contained HTML report from what the runners wrote.
//
//   node report.mjs [out-dir]        (default: ./out; writes <out-dir>/report.html)
import { readFileSync, readdirSync, existsSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const out = process.argv[2] ?? join(here, 'out');
const FRAMEWORKS = [
  ['web', 'Web (Chromium, as in Tauri)'],
  ['qt', 'Qt (PySide6, Fusion)'],
  ['glassmere', 'glassmere'],
];
const present = FRAMEWORKS.filter(([f]) => existsSync(join(out, f)));

const specs = Object.fromEntries(
  readdirSync(join(here, 'scenarios')).filter((f) => f.endsWith('.json'))
    .map((f) => JSON.parse(readFileSync(join(here, 'scenarios', f), 'utf8'))).map((s) => [s.id, s]),
);

// What kind of widget each id is, from the spec.
function kinds(spec) {
  const map = {};
  const walk = (n) => {
    if (n.id) map[n.id] = n.type;
    if (n.id && ((n.width != null && n.height != null) || n.compareBox)) map[`${n.id}#fixed`] = true;
    for (const o of n.options ?? []) map[o.id] = 'radio';
    (n.children ?? []).forEach(walk);
  };
  walk(spec.ui);
  return map;
}
const INTERACTIVE = new Set(['button', 'checkbox', 'switch', 'radio', 'input', 'slider', 'number', 'select']);
const BOXES = new Set(['panel', 'spacer', 'column', 'row', 'scroll', 'stack']);

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const show = (v) => (v === undefined ? '—' : v === null ? 'none' : typeof v === 'object' ? JSON.stringify(v) : String(v));
const stable = (v) => JSON.stringify(v, (_, x) => (x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => a.localeCompare(b))) : x));
const same = (a, b) => stable(a) === stable(b);

function load(fw, id) {
  const file = join(out, fw, `${id}.json`);
  return existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : null;
}
function image(fw, name) {
  const file = join(out, fw, name);
  return existsSync(file) ? `data:image/png;base64,${readFileSync(file).toString('base64')}` : null;
}

// Rows of comparison for one record: [{group, field, values: {fw: v}, differs: {fw: bool}}]
function rowsOf(spec, k, records, unsupported) {
  const rows = [];
  const add = (group, field, values, opts = {}) => {
    const diff = {};
    for (const [fw] of present) {
      if (fw === 'web') continue;
      if (group === 'layout' && unsupported[fw]?.length) { diff[fw] = false; continue; }
      const a = values.web, b = values[fw];
      if (opts.skipNull && (a === null || b === null) && fw === 'qt') { diff[fw] = false; continue; }
      diff[fw] = values.web !== undefined && b !== undefined && !(opts.eq ?? same)(a, b);
    }
    // The comparison that matters most is glassmere against each of the others.
    rows.push({ group, field, values, differs: diff, opts });
  };
  const r = records;
  const type = Object.values(r).find(Boolean)?.type;
  const pick = (f) => Object.fromEntries(present.map(([fw]) => [fw, r[fw]?.[f]]));
  if (type === 'observe') {
    add('behaviour', 'focus', pick('focus'));
    // Only comparable where focus is on the same widget; where it is not, the focus row says so.
    const sameFocus = present.every(([fw]) => r[fw]?.focus === r.web?.focus);
    add('behaviour', 'focus ring shown', sameFocus ? pick('focusVisible') : Object.fromEntries(present.map(([fw]) => [fw, r[fw]?.focus === r.web?.focus ? r[fw]?.focusVisible : undefined])), { skipNull: true });
    add('behaviour', 'values', pick('values'));
    add('behaviour', 'events', pick('log'));
    // How far a wheel notch or an arrow key scrolls differs by design in every toolkit, and the
    // content is as tall as its font makes it; what matters is whether it moved at all.
    add('behaviour', 'scroll offset', pick('scroll'), { eq: (a, b) => Object.keys({ ...a, ...b }).every((i) => (a?.[i] ?? 0) > 0 === ((b?.[i] ?? 0) > 0)), informative: true });
    const ids = Object.keys(Object.values(r).find(Boolean).a11y);
    for (const id of ids) {
      if (INTERACTIVE.has(k[id])) {
        const a11y = Object.fromEntries(present.map(([fw]) => [fw, r[fw]?.a11y?.[id]]));
        add('accessibility', id, a11y, { eq: (a, b) => a && b && a.role === b.role && a.name === b.name && !!a.checked === !!b.checked && !!a.disabled === !!b.disabled && (a.value ?? '') === (b.value ?? '') });
      }
      const rects = Object.fromEntries(present.map(([fw]) => [fw, r[fw]?.rects?.[id]]));
      const fixed = (BOXES.has(k[id]) || k[`${id}#fixed`]) && id !== 'page';
      // Text sets the size of everything else, and fonts differ.
      add('layout', id, rects, { eq: (a, b) => !fixed || (a && b && a.every((v, i) => Math.abs(v - b[i]) <= 2)), informative: !fixed });
    }
  } else if (type === 'cursor') {
    for (const id of Object.keys(Object.values(r).find(Boolean).cursor)) {
      add('pointer', id, Object.fromEntries(present.map(([fw]) => [fw, r[fw]?.cursor?.[id]])));
    }
  } else if (type === 'tabs') {
    add('behaviour', 'focus order', pick('focus'));
  }
  return rows;
}

const scenarios = Object.keys(specs).filter((id) => present.some(([fw]) => load(fw, id))).sort();
// Performance, from each framework's perf.json.
const perf = FRAMEWORKS.map(([fw, name]) => [name, existsSync(join(out, fw, 'perf.json')) ? JSON.parse(readFileSync(join(out, fw, 'perf.json'), 'utf8')) : null]).filter(([, d]) => d);
function perfTable() {
  if (!perf.length) return '';
  const rows = [...new Set(perf.flatMap(([, d]) => Object.values(d.variants).flatMap((v) => Object.keys(v))))].sort((a, b) => a - b);
  const fmt = (v) => (v === undefined ? '—' : v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2));
  let body = '';
  for (const [name, d] of perf) {
    for (const [variant, byRows] of Object.entries(d.variants)) {
      body += `<tr><th>${esc(name)}${Object.keys(d.variants).length > 1 || variant ? ` <span class="muted">${esc(variant)}</span>` : ''}</th>${rows.map((n) => {
        const m = byRows[n];
        return `<td>${m ? `${fmt(m.first)} / ${fmt(m.update)} / ${fmt(m.scroll)}` : '—'}</td>`;
      }).join('')}</tr>`;
    }
  }
  return `<h2>Performance</h2>
<p class="note">A list of N rows (a label, a button and a check box each) in a 400 px scroll area. Each cell is <b>first frame / one label changes / one scroll step</b>, in milliseconds. The web number is main-thread time from the DevTools protocol (script, style, layout, recording the paint; raster runs on other threads and is not counted). Qt's is wall time to lay out and paint the visible part. glassmere's is a whole frame as a host runs it (messages, the view, reconcile, layout, paint) plus a full software raster. All on the same machine in one process, so read the ratios, not the digits.</p>
<table><thead><tr><th>toolkit</th>${rows.map((n) => `<th>${n} rows</th>`).join('')}</tr></thead><tbody>${body}</tbody></table>`;
}
const findings = [];
const limitations = [];
const sections = [];
const summary = [];

for (const id of scenarios) {
  const spec = specs[id];
  const k = kinds(spec);
  const data = Object.fromEntries(present.map(([fw]) => [fw, load(fw, id)]));
  const unsupported = Object.fromEntries(present.map(([fw]) => [fw, data[fw]?.unsupported ?? []]));
  for (const [fw, list] of Object.entries(unsupported)) for (const f of list) limitations.push({ id, fw, feature: f, title: spec.title });
  const count = Math.max(...Object.values(data).map((d) => d?.records.length ?? 0));
  const tally = { qt: 0, glassmere: 0 };
  let html = '';
  for (let i = 0; i < count; i++) {
    const recs = Object.fromEntries(present.map(([fw]) => [fw, data[fw]?.records[i]]));
    const any = Object.values(recs).find(Boolean);
    const rows = rowsOf(spec, k, recs, unsupported);
    const bad = rows.filter((r) => r.differs.glassmere || r.differs.qt);
    for (const row of rows) {
      if (row.differs.glassmere) tally.glassmere++;
      if (row.differs.qt) tally.qt++;
      if (row.differs.glassmere && row.group !== 'layout') findings.push({ id, step: any.label, row });
    }
    const shots = present.map(([fw, name]) => {
      const file = data[fw]?.screenshots?.[data[fw].records.slice(0, i + 1).filter((r) => r.type === 'observe').length - 1];
      const src = recs[fw]?.type === 'observe' && file ? image(fw, file) : null;
      return src ? `<figure><img src="${src}" alt=""><figcaption>${esc(name)}</figcaption></figure>` : '';
    }).join('');
    const table = rows.map((row) => {
      const cells = present.map(([fw]) => {
        const v = row.values[fw];
        const cls = fw !== 'web' && row.differs[fw] ? ' class="differs"' : '';
        return `<td${cls}>${esc(show(v))}</td>`;
      }).join('');
      const rowCls = row.differs.glassmere || row.differs.qt ? 'bad' : row.opts.informative ? 'info' : 'ok';
      return `<tr class="${rowCls}"><td class="g">${row.group}</td><th>${esc(row.field)}</th>${cells}</tr>`;
    }).join('');
    html += `<details${bad.length ? ' open' : ''} class="step ${bad.length ? 'has' : ''}"><summary>${esc(any.label)} <span class="badge">${bad.length ? `${bad.length} differ` : 'same'}</span></summary>
      ${shots ? `<div class="shots">${shots}</div>` : ''}
      <table><thead><tr><th>kind</th><th>what</th>${present.map(([, n]) => `<th>${esc(n)}</th>`).join('')}</tr></thead><tbody>${table}</tbody></table></details>`;
  }
  summary.push({ id, title: spec.title, ...tally });
  sections.push(`<section id="${id}"><h2>${esc(spec.title)}</h2><p class="id">${id}</p>${html}</section>`);
}

const findingRows = findings.map(({ id, step, row }) => {
  const w = row.values.web, g = row.values.glassmere, q = row.values.qt;
  return `<tr><td><a href="#${id}">${id}</a></td><td>${esc(step)}</td><th>${esc(row.field)}</th><td>${esc(show(w))}</td><td>${esc(show(q))}</td><td class="differs">${esc(show(g))}</td></tr>`;
}).join('');

const page = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Parity bench</title><style>
:root { --bg:#11151d; --panel:#1a202c; --line:#2b3446; --text:#e6ebf5; --muted:#93a0b8; --bad:#f2b84b; --ok:#4cc38a; }
@media (prefers-color-scheme: light) { :root { --bg:#f6f7fa; --panel:#fff; --line:#d9dee8; --text:#1c2330; --muted:#5a667b; --bad:#a85d00; --ok:#18794e; } }
* { box-sizing: border-box; } body { margin:0; background:var(--bg); color:var(--text); font:14px/1.45 system-ui,sans-serif; }
main { max-width:1180px; margin:0 auto; padding:24px 16px 80px; } h1 { margin:0 0 4px; font-size:24px; } h2 { margin:32px 0 2px; font-size:18px; }
.id { color:var(--muted); margin:0 0 8px; font:12px ui-monospace,monospace; } p.lede { color:var(--muted); max-width:760px; }
table { width:100%; border-collapse:collapse; background:var(--panel); border:1px solid var(--line); margin:8px 0 16px; font-size:13px; }
th,td { text-align:left; padding:5px 8px; border-bottom:1px solid var(--line); vertical-align:top; word-break:break-word; } thead th { color:var(--muted); font-weight:600; }
td.g { color:var(--muted); width:96px; } .muted { color:var(--muted); font-weight:400; } td.differs { color:var(--bad); font-weight:600; } tr.info td, tr.info th { color:var(--muted); }
details.step { background:var(--panel); border:1px solid var(--line); border-radius:8px; margin:8px 0; padding:0 12px; } details.step table { margin-top:0; }
summary { cursor:pointer; padding:10px 0; font-weight:600; } .badge { font-weight:400; font-size:12px; margin-left:8px; color:var(--ok); } .has .badge { color:var(--bad); }
.shots { display:flex; flex-wrap:wrap; gap:12px; margin:4px 0 12px; } figure { margin:0; flex:1 1 240px; max-width:400px; } img { display:block; border:1px solid var(--line); width:100%; height:auto; }
td:first-child a, th:first-child a { white-space:nowrap; } figcaption { color:var(--muted); font-size:12px; margin-top:4px; }
.toolbar { margin:12px 0; color:var(--muted); } label { cursor:pointer; } .note { color:var(--muted); font-size:13px; max-width:760px; }
.only-diff details.step:not(.has) { display:none; } .only-diff tr.ok { display:none; }
</style></head><body><main>
<h1>glassmere parity bench</h1>
<p class="lede">The same scenarios, built three ways and driven with the same mouse and keyboard steps. The web column is Chromium, the engine behind Tauri's WebView2 on Windows; Qt is PySide6 with the Fusion style on the offscreen platform. Amber cells are where a column differs from the web.</p>
<table><thead><tr><th>scenario</th><th>what it checks</th><th>Qt differs from web</th><th>glassmere differs from web</th></tr></thead><tbody>
${summary.map((s) => `<tr><th><a href="#${s.id}">${s.id}</a></th><td>${esc(s.title)}</td><td>${s.qt}</td><td>${s.glassmere}</td></tr>`).join('')}
</tbody></table>
<h2>Where glassmere differs from the web, in behaviour</h2>
<p class="note">Layout rows are left out here and shown in each scenario below: fonts differ between frameworks, so only boxes with a fixed size are compared.</p>
<table><thead><tr><th>scenario</th><th>step</th><th>what</th><th>web</th><th>Qt</th><th>glassmere</th></tr></thead><tbody>${findingRows || '<tr><td colspan="6">No differences.</td></tr>'}</tbody></table>
${perfTable()}
<h2>What a toolkit cannot express</h2>
<p class="note">Scenarios that need a property the toolkit does not have. The property is left out there and its layout rows are not compared.</p>
<table><thead><tr><th>scenario</th><th>toolkit</th><th>property</th></tr></thead><tbody>${limitations.map((l) => `<tr><td><a href="#${l.id}">${l.id}</a></td><th>${esc(FRAMEWORKS.find(([f]) => f === l.fw)[1])}</th><td>${esc(l.feature)}</td></tr>`).join('') || '<tr><td colspan="3">None.</td></tr>'}</tbody></table>
<div class="toolbar"><label><input type="checkbox" id="only"> Show only what differs</label></div>
<div id="all">${sections.join('')}</div>
<h2>How to read this</h2>
<p class="note">Observations come from the framework itself: Chromium's accessibility tree and <code>:focus-visible</code>, Qt's <code>QAccessible</code>, and glassmere's snapshots and scene. A pointer is reported as the framework would show it; for the web an <code>auto</code> cursor is read as an I-beam over text and the arrow elsewhere. Qt draws its focus frame from the style and cannot be asked, so its ring is not compared. Qt's first control takes focus when its window is shown, as in a real application. Qt Widgets has no switch, so the switch is a check box there.</p>
</main><script>
const only = document.getElementById('only'), all = document.getElementById('all');
only.addEventListener('change', () => all.classList.toggle('only-diff', only.checked));
</script></body></html>`;

if (process.argv.includes('--list')) {
  for (const { id, step, row } of findings) {
    const v = row.values;
    console.log(`${id} | ${step} | ${row.field}\n    web=${show(v.web)}\n    qt=${show(v.qt)}\n    gm=${show(v.glassmere)}`);
  }
}
writeFileSync(join(out, 'report.html'), page);
console.log(`report: ${scenarios.length} scenarios, ${findings.length} behavioural differences -> ${join(out, 'report.html')}`);
