#!/usr/bin/env node
// Drive the explorer page in a real headless Chrome, for the measurement ADR 0054 asks for (unit E-1). Not part of the gate:
// the gate has no browser (the named gap). Node's own WebSocket and the Chrome DevTools protocol; no package, no download.
//
//   node tools/explorer/spike/browser.mjs measure <page.html> [--limit N] [--runs R]    one JSON line per measurement
//   node tools/explorer/spike/browser.mjs keys <page.html>        Tab through the controls; Enter on a node (real key events)
//   node tools/explorer/spike/browser.mjs probe <page.html> <js> [--click <data-k>]...   print JSON of <js> after the clicks
//   node tools/explorer/spike/browser.mjs shot <page.html> <out.png> [--width W] [--height H] [--mobile]
//        [--hash '#focus=REQ-0001'] [--click <data-k>]... [--dark]
//
// The browser is whatever ARCHIFY_CHROME names, else the macOS Google Chrome (archify's visual-check finds the same one).
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const CHROME = process.env.ARCHIFY_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

async function launch() {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'req-explorer-chrome-'));
  const child = spawn(CHROME, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run',
    '--no-default-browser-check', '--disable-gpu', '--hide-scrollbars', '--allow-file-access-from-files', 'about:blank'],
  { stdio: ['ignore', 'ignore', 'pipe'] });
  const url = await new Promise((resolve, reject) => {
    let text = '';
    child.stderr.on('data', (chunk) => {
      text += chunk;
      const m = /DevTools listening on (ws:\/\/\S+)/.exec(text);
      if (m) resolve(m[1]);
    });
    child.on('exit', (code) => reject(new Error(`chrome exited ${code}: ${text}`)));
    setTimeout(() => reject(new Error('chrome did not start in 30 s')), 30000);
  });
  const ws = new WebSocket(url);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let id = 0;
  const pending = new Map();
  const listeners = [];
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    } else listeners.forEach((fn) => fn(msg));
  };
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const n = ++id;
    pending.set(n, { resolve, reject });
    ws.send(JSON.stringify({ id: n, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const page = (method, params) => send(method, params, sessionId);
  await page('Page.enable');
  await page('Runtime.enable');
  const close = async () => {
    const gone = new Promise((resolve) => child.once('exit', resolve));
    try { await send('Browser.close'); } catch (e) { /* gone */ }
    child.kill();
    await Promise.race([gone, new Promise((resolve) => setTimeout(resolve, 5000))]);
    fs.rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 });
  };
  return { page, listeners, close };
}

async function evaluate(page, expression) {
  const r = await page('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
}

async function open(b, file, options = {}) {
  await b.page('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: options.dark ? 'dark' : 'light' }] });
  await b.page('Emulation.setDeviceMetricsOverride', { width: options.width || 1440, height: options.height || 900, deviceScaleFactor: 1, mobile: !!options.mobile });
  const loaded = new Promise((resolve) => b.listeners.push((m) => { if (m.method === 'Page.loadEventFired') resolve(); }));
  await b.page('Page.navigate', { url: pathToFileURL(file).href + (options.hash || '') });
  await loaded;
  for (let i = 0; i < 100; i++) {
    const ready = await evaluate(b.page, "performance.getEntriesByName('req-explorer-first-draw').length");
    if (ready) break;
    await new Promise((r) => setTimeout(r, 50));
  }
}

// One interaction: the script runs in the page; the time is from the dispatch to the second frame after it (so layout and
// paint of the new SVG are in it), taken with the page's own clock.
const HELPERS = `
  window.__t = async function (fn) {
    var t0 = performance.now();
    fn();
    await new Promise(function (r) { requestAnimationFrame(function () { requestAnimationFrame(r); }); });
    return +(performance.now() - t0).toFixed(1);
  };
  window.__state = function () {
    var s = document.getElementById('status');
    return { nodes: document.querySelectorAll('#graph .n').length, hidden: /Narrow the view/.test(s.textContent), status: s.textContent.slice(0, 110) };
  };
  window.__set = function (id, value) { var e = document.getElementById(id); e.value = value; e.dispatchEvent(new Event(e.id === 'q' ? 'input' : 'change', { bubbles: true })); };
  window.__tick = function (id) { var e = document.getElementById(id); e.checked = !e.checked; e.dispatchEvent(new Event('change', { bubbles: true })); };
  window.__click = function (sel) { var e = document.querySelector(sel); if (!e) throw new Error('no ' + sel); e.dispatchEvent(new MouseEvent('click', { bubbles: true })); };
`;

async function measure(file, flags) {
  const b = await launch();
  try {
    const bytes = fs.statSync(file).size;
    await open(b, file);
    const first = await evaluate(b.page, "performance.getEntriesByName('req-explorer-first-draw')[0].startTime");
    await evaluate(b.page, HELPERS);
    const initial = await evaluate(b.page, 'window.__state()');
    const info = await evaluate(b.page, `(function(){ var d = JSON.parse(document.getElementById('req-explorer-data').textContent);
      return { claims: d.claims.length, areas: d.areas.length, design: d.design.length, sources: d.sources.length, groups: d.tests.length,
               links: d.edges.satisfied_by.length, gaps: d.gaps.length, firstArea: d.areas[0] && d.areas[0].path,
               outcomes: Array.from(new Set(d.edges.satisfied_by.map(function (e) { return e.outcome; }))).sort() }; })()`);
    const results = [];
    const run = async (label, script, after = 'document.getElementById("reset").click()') => {
      const times = [];
      let last;
      for (let i = 0; i < (flags.runs || 3); i++) {
        times.push(await evaluate(b.page, `window.__t(function(){ ${script} })`));
        last = await evaluate(b.page, 'window.__state()');
        await evaluate(b.page, after);
      }
      times.sort((x, y) => x - y);
      results.push({ interaction: label, ms: times[Math.floor(times.length / 2)], max_ms: times[times.length - 1], nodes: last.nodes, hidden: last.hidden });
    };
    const area0 = info.firstArea;
    await run('expand one area', `window.__click('[data-k="a:${area0}"]')`);
    // The baseline of every later interaction: one area open (the reset closes it again, so each run re-opens it).
    const OPEN = `window.__click('[data-k="a:${area0}"]')`;
    await evaluate(b.page, OPEN);
    const k = await evaluate(b.page, `(function(){ var c = document.querySelector('#graph .n-claim'); return c ? c.getAttribute('data-k') : null; })()`);
    const d = await evaluate(b.page, `(function(){ var c = document.querySelector('#graph .n-design'); return c ? c.getAttribute('data-k') : null; })()`);
    await run('focus one claim (area open)', `window.__click('[data-k="${k}"]')`, `document.querySelector('#clear-focus') && document.querySelector('#clear-focus').click()`);
    if (d) await run('focus one design node', `window.__click('[data-k="${d}"]')`, `document.querySelector('#clear-focus') && document.querySelector('#clear-focus').click()`);
    await evaluate(b.page, 'document.getElementById("reset").click()');
    await evaluate(b.page, 'document.getElementById("reset").click()');
    await evaluate(b.page, OPEN);
    await run('layer off: source', `window.__tick('l-source')`, `window.__tick('l-source')`);
    await run('layer off: design', `window.__tick('l-design')`, `window.__tick('l-design')`);
    await run('layer off: tests', `window.__tick('l-tests')`, `window.__tick('l-tests')`);
    await evaluate(b.page, 'document.getElementById("reset").click()');
    for (const o of info.outcomes) await run(`outcome = ${o}`, `window.__set('outcome', '${o}')`);
    await run(`area = ${area0}`, `window.__set('area', '${area0}')`);
    await run('gaps only', `window.__tick('gaps')`, `window.__tick('gaps')`);
    await run('search by claim id (a prefix, REQ-00)', `window.__set('q', 'REQ-00')`, `window.__set('q', '')`);
    await run('search by claim id (one, REQ-0042)', `window.__set('q', 'REQ-0042')`, `window.__set('q', '')`);
    const element = await evaluate(b.page, `JSON.parse(document.getElementById('req-explorer-data').textContent).design[0].id`);
    await run('search by file path', `window.__set('q', ${JSON.stringify(element)})`, `window.__set('q', '')`);
    const testId = await evaluate(b.page, `JSON.parse(document.getElementById('req-explorer-data').textContent).tests[0].ids[0]`);
    await run('search by test id', `window.__set('q', ${JSON.stringify(testId)})`, `window.__set('q', '')`);
    console.log(JSON.stringify({ file: path.basename(file), bytes, first_draw_ms: +first.toFixed(1), initial, info, results }));
  } finally { await b.close(); }
}

async function shot(file, out, flags) {
  const b = await launch();
  try {
    await open(b, file, flags);
    await evaluate(b.page, HELPERS);
    for (const sel of flags.click || []) await evaluate(b.page, `window.__click(${JSON.stringify(sel)})`);
    await new Promise((r) => setTimeout(r, 300));
    const { data } = await b.page('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(out, Buffer.from(data, 'base64'));
    console.log(`wrote ${out}`);
  } finally { await b.close(); }
}

async function keys(file) {
  const b = await launch();
  try {
    await open(b, file, {});
    const press = async (key, code, vk) => {
      for (const type of ['rawKeyDown', 'keyUp']) await b.page('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode: vk, text: key === 'Enter' ? '\r' : undefined });
    };
    const describe = `(function(){ var a = document.activeElement; return a ? (a.id ? '#' + a.id : a.getAttribute('data-k') ? '[' + a.getAttribute('data-k') + ']' : a.tagName + (a.getAttribute('href') ? ' href' : '')) : null; })()`;
    const reached = [];
    for (let i = 0; i < 40; i++) { await press('Tab', 'Tab', 9); reached.push(await evaluate(b.page, describe)); }
    const controls = ['#q', '#l-source', '#l-design', '#l-tests', '#outcome', '#area', '#gaps', '#reset'];
    const missing = controls.filter((c) => !reached.includes(c));
    // Enter on an area row opens it; Enter on a claim focuses it (the keyboard path, no pointer).
    await evaluate(b.page, `document.querySelector('[data-k^="a:"]').focus()`);
    await press('Enter', 'Enter', 13);
    const afterArea = await evaluate(b.page, `document.querySelectorAll('#graph .n-claim').length`);
    await evaluate(b.page, `document.querySelector('#graph .n-claim').focus()`);
    await press('Enter', 'Enter', 13);
    const focused = await evaluate(b.page, `document.querySelector('#graph .n.focus') ? document.querySelector('#graph .n.focus').getAttribute('data-k') : null`);
    const kept = await evaluate(b.page, describe);
    await press('Escape', 'Escape', 27);
    const cleared = await evaluate(b.page, `document.querySelector('#graph .n.focus') === null`);
    console.log(JSON.stringify({ tab_reached: reached, controls_missing: missing, claims_after_enter_on_area: afterArea, focused_after_enter: focused, keyboard_focus_kept_on: kept, escape_clears: cleared }));
  } finally { await b.close(); }
}

async function probe(file, expression, flags) {
  const b = await launch();
  try {
    await open(b, file, flags);
    await evaluate(b.page, HELPERS);
    for (const sel of flags.click || []) await evaluate(b.page, `window.__click(${JSON.stringify(sel)})`);
    console.log(JSON.stringify(await evaluate(b.page, expression)));
  } finally { await b.close(); }
}

const [command, file, ...rest] = process.argv.slice(2);
const flags = { click: [] };
const positional = [];
for (let i = 0; i < rest.length; i++) {
  const a = rest[i];
  if (a === '--width') flags.width = +rest[++i];
  else if (a === '--height') flags.height = +rest[++i];
  else if (a === '--mobile') flags.mobile = true;
  else if (a === '--dark') flags.dark = true;
  else if (a === '--hash') flags.hash = rest[++i];
  else if (a === '--click') flags.click.push(`[data-k="${rest[++i]}"]`);
  else if (a === '--runs') flags.runs = +rest[++i];
  else positional.push(a);
}
if (command === 'measure' && file) await measure(path.resolve(file), flags);
else if (command === 'probe' && file && positional[0]) await probe(path.resolve(file), positional[0], flags);
else if (command === 'keys' && file) await keys(path.resolve(file));
else if (command === 'shot' && file && positional[0]) await shot(path.resolve(file), path.resolve(positional[0]), flags);
else { console.error('usage: browser.mjs measure <page> | shot <page> <out.png> [--width W --height H --mobile --hash H --click K --dark]'); process.exit(2); }
