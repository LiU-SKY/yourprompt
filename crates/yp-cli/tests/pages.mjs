// The two web pages, run in Node against a stand-in for the browser.
//
// Each page is one inline script, and CI only parses it (`node --check`), so a
// script that parses and then throws text away passes every check there is.
// This runs the script itself. The elements it looks up, the events it
// listens for and the few browser APIs it calls are replaced by the small
// fakes below, and each check plays one visitor's action against the page's
// own code and reads back what the page would show. Layout is modelled only
// as far as the checks need it: a 14 px monospace font, wrapped on spaces,
// which puts the long example on seven lines in a 309 px box, as Chrome does.
//
// Usage: node pages.mjs <docs/index.html> <crates/yp-cli/assets/index.html> [check]

import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const [demoPath, servePath, only] = process.argv.slice(2);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
// Long enough for a file read and a level of folder entries to come back.
const settle = () => sleep(40);

// The prompt box: 14px/1.6 monospace, 12 px of padding, a 1 px border.
const CHAR = 8.4, LINE = 22.4, PAD = 12, BORDER = 1;

// How many lines a box `width` px wide wraps `text` into.
function wrapped(text, width) {
  const cols = Math.max(1, Math.floor((width - 2 * (PAD + BORDER)) / CHAR));
  let lines = 0;
  for (const hard of String(text).split('\n')) {
    let used = 0, count = 1;
    for (const word of hard.split(' ')) {
      let len = word.length;
      if ((used ? used + 1 : 0) + len <= cols) { used += (used ? 1 : 0) + len; continue; }
      if (used) count++;
      while (len > cols) { count++; len -= cols; }
      used = len;
    }
    lines += count;
  }
  return lines;
}
const textHeight = (text, width) => Math.round(wrapped(text, width) * LINE + 2 * PAD);

function attrsOf(text) {
  const attrs = {};
  for (const m of text.matchAll(/([\w-]+)(?:="([^"]*)")?/g)) attrs[m[1]] = m[2] ?? '';
  return attrs;
}

class Ev {
  constructor(type, init) {
    Object.assign(this, init);
    this.type = type;
    this.defaultPrevented = false;
  }
  preventDefault() { this.defaultPrevented = true; }
}

class El {
  constructor(page, tag, attrs = {}, text = '') {
    this.page = page;
    this.tagName = tag.toUpperCase();
    this.attrs = attrs;
    this.id = attrs.id || '';
    this.dataset = {};
    for (const [key, value] of Object.entries(attrs)) {
      if (key.startsWith('data-')) {
        this.dataset[key.slice(5).replace(/-(\w)/g, (_, c) => c.toUpperCase())] = value;
      }
    }
    this.classes = new Set((attrs.class || '').split(/\s+/).filter(Boolean));
    this.style = { cssText: '' };
    this.listeners = {};
    this.parentNode = null;
    this.children = [];
    this.parsed = [];
    this.markup = '';
    this.textContent = text;
    this.value = '';
    this.placeholder = '';
    this.hidden = 'hidden' in attrs;
    this.disabled = 'disabled' in attrs;
    this.scrollTop = 0;
    // A desktop box until a check says otherwise.
    this.offsetWidth = 886; this.clientWidth = 884;
    this.offsetHeight = 128; this.clientHeight = 126;
    if (this.tagName === 'INPUT') this.webkitdirectory = 'webkitdirectory' in attrs;
  }
  get className() { return [...this.classes].join(' '); }
  set className(value) { this.classes = new Set(String(value).split(/\s+/).filter(Boolean)); }
  get classList() {
    const c = this.classes;
    return {
      add: name => c.add(name),
      remove: name => c.delete(name),
      contains: name => c.has(name),
      toggle: (name, on = !c.has(name)) => (on ? c.add(name) : c.delete(name), on),
    };
  }
  // The page builds its buttons as markup; those are what a check clicks.
  get innerHTML() { return this.markup; }
  set innerHTML(html) {
    this.markup = String(html);
    this.parsed = [...this.markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)].map(m =>
      new El(this.page, 'button', attrsOf(m[1]), m[2].replace(/<[^>]*>/g, '')));
    for (const el of this.parsed) el.parentNode = this;
  }
  querySelectorAll(selector) {
    const m = /^(\w+)?(?:\[([\w-]+)(?:="([^"]*)")?\])?$/.exec(selector.trim());
    if (!m) throw new Error('selector not modelled: ' + selector);
    const [, tag, attr, value] = m;
    return this.parsed.filter(el => (!tag || el.tagName === tag.toUpperCase()) &&
      (!attr || (attr in el.attrs && (value === undefined || el.attrs[attr] === value))));
  }
  closest(selector) {
    const m = /^\[([\w-]+)="([^"]*)"\]$/.exec(selector);
    return m && this.attrs[m[1]] === m[2] ? this : null;
  }
  // What a hidden copy of the prompt box measures: its text at its width.
  get scrollHeight() {
    const width = /width:\s*([\d.]+)px/.exec(this.style.cssText);
    return textHeight(this.value, width ? Number(width[1]) : this.offsetWidth);
  }
  addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
  // Listeners here first, then the document's: the page delegates one click.
  dispatch(type, init = {}) {
    const event = new Ev(type, { target: this, ...init });
    for (const fn of this.listeners[type] || []) fn.call(this, event);
    const doc = this.page.document;
    if (doc && this !== doc) for (const fn of doc.listeners[type] || []) fn.call(doc, event);
    return event;
  }
  click() { return this.dispatch('click'); }
  appendChild(child) { child.parentNode = this; this.children.push(child); return child; }
  remove() {
    if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(c => c !== this);
    this.parentNode = null;
  }
  setAttribute(name, value) { this.attrs[name] = String(value); }
  getAttribute(name) { return name in this.attrs ? this.attrs[name] : null; }
  focus() {}
  setSelectionRange() {}
}

// A file as the picker or a drop hands it over, read back by the fake reader.
const fakeFile = (path, text = 'pub fn verify_token() {}\n') => ({
  name: path.split('/').pop(), size: Buffer.byteLength(text), text, webkitRelativePath: path,
});

// A dropped folder, as the entries API hands it over: one batch of children
// per directory, then an empty one.
function folder(root, inside) {
  const dir = (name, fullPath) => ({
    isDirectory: true, isFile: false, name, fullPath, kids: new Map(),
    createReader() {
      let given = false;
      return { readEntries: ok => setTimeout(() => { ok(given ? [] : [...this.kids.values()]); given = true; }) };
    },
  });
  const top = dir(root, '/' + root);
  for (const path of inside) {
    let at = top;
    const parts = path.split('/');
    parts.forEach((part, i) => {
      const fullPath = `${at.fullPath}/${part}`;
      if (i === parts.length - 1) {
        at.kids.set(part, { isDirectory: false, isFile: true, name: part, fullPath, file: ok => ok(fakeFile(part)) });
      } else {
        if (!at.kids.has(part)) at.kids.set(part, dir(part, fullPath));
        at = at.kids.get(part);
      }
    });
  }
  return top;
}

const drag = (types, items = []) => ({ dataTransfer: { types, items, files: [] } });
const drop = (page, folders) => page.$('atts').dispatch('drop',
  drag(['Files'], folders.map(entry => ({ kind: 'file', webkitGetAsEntry: () => entry }))));
const pick = (page, root, inside) => {
  page.$('folder').files = inside.map(path => fakeFile(`${root}/${path}`));
  page.$('folder').dispatch('change');
};

// What the scorer returns, in the shapes the checks need.
const axis = id => ({ id, earned: 100, max: 250, components: [] });
const GRADED = {
  kind: 'task', language: null, total: 799.8, grade: 'A-', renormalized: true, grounding: null,
  actionability: axis('actionability'), clarity: axis('clarity'), context: axis('context'), marks: [],
};
const FOLLOW_UP = { ...GRADED, kind: 'follow_up', total: 65.0, grade: 'F' };

// Open a page: its elements, its script run once, as at a visit.
function open(path, { lang = 'en-US', info = { repo: '/repo', files: 6 } } = {}) {
  const html = readFileSync(path, 'utf8');
  const script = html.slice(html.indexOf('<script>') + 8, html.lastIndexOf('</script>'));
  const page = { listeners: {}, downloads: [], observers: [], clipboard: null };
  const byId = new Map();
  for (const m of html.matchAll(/<(\w+)\b([^>]*\bid="([^"]+)"[^>]*)>/g)) {
    byId.set(m[3], new El(page, m[1], attrsOf(m[2])));
  }
  const editor = new El(page, 'div');
  for (const el of byId.values()) el.parentNode = editor;
  const document = new El(page, '#document');
  Object.assign(document, {
    getElementById: id => byId.get(id) || null,
    createElement: tag => new El(page, tag),
    documentElement: new El(page, 'html'),
    body: new El(page, 'body'),
    title: '',
  });
  page.document = document;
  page.location = { origin: 'http://127.0.0.1:8787', pathname: '/', search: '', hash: '' };
  const storage = new Map();
  const context = {
    document, console, Blob, TextEncoder, TextDecoder, atob, btoa, setTimeout, clearTimeout,
    WebAssembly,
    location: page.location,
    history: {
      replaceState: (_state, _title, url) => {
        const at = String(url).indexOf('#');
        page.location.hash = at < 0 ? '' : String(url).slice(at);
      },
    },
    navigator: { language: lang, clipboard: { writeText: async text => { page.clipboard = text; } } },
    localStorage: {
      getItem: key => (storage.has(key) ? storage.get(key) : null),
      setItem: (key, value) => storage.set(key, String(value)),
    },
    getComputedStyle: el => ({
      minHeight: el.style.minHeight || '128px', lineHeight: LINE + 'px',
      paddingTop: PAD + 'px', paddingBottom: PAD + 'px',
    }),
    ResizeObserver: class {
      constructor(fn) { this.fn = fn; this.targets = []; page.observers.push(this); }
      observe(el) { this.targets.push(el); }
    },
    FileReader: class {
      readAsText(file) { setTimeout(() => { this.result = file.text; this.onload(); }); }
    },
    // No scorer and no star count; the serve page's two routes answer.
    fetch: async url => {
      const body = { '/api/info': info, '/api/score': GRADED }[String(url)];
      return body ? { ok: true, status: 200, json: async () => body } : { ok: false, status: 404 };
    },
    URL: {
      createObjectURL: blob => { page.downloads.push(blob); return 'blob:' + page.downloads.length; },
      revokeObjectURL: () => {},
    },
  };
  context.window = context;
  context.addEventListener = (type, fn) => (page.listeners[type] ||= []).push(fn);
  vm.createContext(context);
  vm.runInContext(script, context, { filename: path });
  page.$ = id => byId.get(id);
  page.run = code => vm.runInContext(code, context);
  // Values made inside the page come back through JSON, as plain data.
  page.get = code => JSON.parse(vm.runInContext(`JSON.stringify(${code})`, context));
  page.fire = type => { for (const fn of page.listeners[type] || []) fn(new Ev(type, {})); };
  return page;
}

const SRC = ['src/auth/login.rs', 'src/auth/token.rs', 'src/routes.rs', 'src/session.rs', 'tests/auth.rs'];

const checks = {
  // Text dragged onto the prompt box is the textarea's to insert; a drag that
  // carries files is the page's. The file list keeps any drag on the page,
  // and lights up for files only. Both pages.
  'text-drop': async () => {
    for (const path of [demoPath, servePath]) {
      const page = open(path);
      for (const id of ['p', 'atts']) {
        const el = page.$(id);
        for (const type of ['dragenter', 'dragover', 'drop']) {
          const event = el.dispatch(type, drag(['text/plain']));
          assert.equal(event.defaultPrevented, id === 'atts', `${path} #${id}: a text ${type}`);
          assert.ok(!el.classList.contains('dropping'), `${path} #${id}: lit up for text`);
        }
        assert.equal(page.$('note').textContent, '', `${path} #${id}: a text drop said something`);
        const event = el.dispatch('dragover', drag(['Files']));
        assert.equal(event.defaultPrevented, true, `${path} #${id}: a file drag was not taken`);
        assert.ok(el.classList.contains('dropping'), `${path} #${id}: not lit up for files`);
        assert.equal(el.dispatch('drop', drag(['Files'])).defaultPrevented, true);
      }
    }
  },

  // A single folder is indexed under the paths inside it unless its own name
  // is part of the paths a prompt uses, chosen with the picker or dropped.
  'lone-folder': async () => {
    const cases = [
      // what is chosen, what is in it, the names it is indexed under
      ['nomarker', SRC, SRC],
      ['acme-auth', ['Cargo.toml', ...SRC], ['Cargo.toml', ...SRC]],
      ['auth', ['login.rs', 'token.rs'], ['login.rs', 'token.rs']],
      ['src', ['auth/login.rs', 'routes.rs'], ['src/auth/login.rs', 'src/routes.rs']],
      ['tests', ['auth.rs'], ['tests/auth.rs']],
      ['Lib', ['util.rs'], ['Lib/util.rs']],
      ['app', ['src/main.rs', 'tests/it.rs'], ['src/main.rs', 'tests/it.rs']],
    ];
    for (const [root, inside, names] of cases) {
      for (const how of ['picked', 'dropped']) {
        const page = open(demoPath);
        if (how === 'picked') pick(page, root, inside);
        else drop(page, [folder(root, inside)]);
        for (let i = 0; i < 5; i++) await settle();
        assert.deepEqual(page.get('files.map(f => f.name)').sort(), [...names].sort(), `${root}/ ${how}`);
      }
    }
    const page = open(demoPath);
    drop(page, [folder('src', ['auth/login.rs']), folder('tests', ['auth.rs'])]);
    for (let i = 0; i < 5; i++) await settle();
    assert.deepEqual(page.get('files.map(f => f.name)').sort(), ['src/auth/login.rs', 'tests/auth.rs']);
  },

  // Removing what was loaded says so, instead of leaving "loaded the sample
  // repo, 6 files" over an empty list.
  'removed-note': async () => {
    const said = {
      en: ['removed the sample repo', 'removed nomarker/', 'removed notes.txt'],
      ko: ['샘플 저장소를 제거했습니다', 'nomarker/ 제거됨', 'notes.txt 제거됨'],
    };
    for (const [lang, [sample, dropped, attached]] of Object.entries(said)) {
      const page = open(demoPath);
      page.run(`setLang('${lang}')`);
      page.$('atts').querySelectorAll('button[data-act="sample"]')[0].click();
      assert.equal(page.get('files.length'), 6);
      page.$('atts').querySelectorAll('button[data-row]')[0].click();
      assert.equal(page.get('files.length'), 0);
      assert.equal(page.$('note').textContent, sample, `${lang}: the sample removed`);
      pick(page, 'nomarker', SRC);
      await settle();
      page.$('atts').querySelectorAll('button[data-row]')[0].click();
      assert.equal(page.$('note').textContent, dropped, `${lang}: a folder removed`);

      const served = open(servePath);
      served.run(`setLang('${lang}')`);
      served.$('file').files = [fakeFile('notes.txt', 'Keep verify_token.\n')];
      served.$('file').dispatch('change');
      await settle();
      served.$('atts').querySelectorAll('button')[0].click();
      assert.equal(served.get('attachments.length'), 0);
      assert.equal(served.$('note').textContent, attached, `${lang}: an attachment removed`);
    }
  },

  // A link carries the prompt and the sample repository, never the visitor's
  // own files, and the note after Copy link says which of them it carries.
  'link-note': async () => {
    const leftOut = { en: /your (own )?files are not/, ko: /추가한 파일은 담기지 않/ };
    for (const lang of ['en', 'ko']) {
      for (const load of [[], ['sample'], ['own'], ['sample', 'own']]) {
        const page = open(demoPath);
        page.run(`setLang('${lang}')`);
        if (load.includes('sample')) page.run('loadSample()');
        if (load.includes('own')) { pick(page, 'acme-auth', ['Cargo.toml', ...SRC]); await settle(); }
        page.$('p').value = 'fix the bug';
        page.$('share').click();
        await settle();
        const note = page.$('note').textContent, what = `${lang}, ${load.join(' and ') || 'nothing'} loaded`;
        assert.equal(leftOut[lang].test(note), load.includes('own'), `${what}: "${note}"`);
        const key = load.includes('own') ? (load.includes('sample') ? 'copied_sample_own' : 'copied_own')
          : (load.includes('sample') ? 'copied_sample' : 'copied');
        assert.equal(note, page.run(`STR.${lang}.${key}`), what);
        assert.equal(page.clipboard.includes('repo=sample'), load.includes('sample'), what);
      }
    }
  },

  // The marks are realigned when the box changes size without a keystroke:
  // a window resize, or its own resize handle.
  'resize': async () => {
    for (const path of [demoPath, servePath]) {
      const page = open(path);
      const ta = page.$('p'), hl = page.$('hl');
      const watching = page.observers.filter(o => o.targets.includes(ta));
      assert.ok(watching.length, `${path}: nothing watches the prompt box change size`);
      ta.offsetWidth = 900; ta.clientWidth = 883;          // a 15 px scrollbar appeared
      for (const observer of watching) observer.fn([]);
      assert.equal(hl.style.paddingRight, '27px', path);
    }
  },

  // A prompt the page puts in the box, an example or a shared one, shows in
  // full, up to ten lines; Clear gives the box back to its placeholder.
  'box-fit': async () => {
    const width = 309;                                   // the box on a 375 px phone
    const cap = Math.round(10 * LINE + 2 * PAD);
    for (const lang of ['en', 'ko']) {
      const page = open(demoPath);
      page.run(`setLang('${lang}')`);
      const ta = page.$('p');
      ta.offsetWidth = width;
      page.fire('resize');
      const room = () => parseFloat(ta.style.minHeight || '128') - 2;
      const placeholder = textHeight(ta.placeholder, width);
      assert.ok(room() >= placeholder, `${lang}: the placeholder is cut`);
      const examples = page.get(`STR.${lang}.examples`);
      examples.forEach(([label, text = label], i) => {
        ta.value = '';
        ta.dispatch('input');
        page.$('try').querySelectorAll('button')[i].click();
        assert.equal(ta.value, text);
        assert.ok(room() >= textHeight(text, width), `${lang}: "${label}" is cut at ${room()} px`);
      });
      const long = Array.from({ length: 30 }, (_, i) => `line ${i} of a pasted trace`).join('\n');
      page.location.hash = '#p=' + page.run(`encodePrompt(${JSON.stringify(long)})`);
      page.fire('hashchange');
      assert.equal(ta.value, long);
      assert.equal(room(), cap, `${lang}: a long shared prompt is not held to ten lines`);
      page.$('clear').click();
      assert.equal(room(), Math.max(126, placeholder), `${lang}: Clear left the box tall`);
    }
  },

  // A dangling pronoun is one whose name, if any, is not in the code: the
  // label said "nothing named before it" right after gravity(). The two
  // pages label every mark alike.
  'pronoun-label': async () => {
    const labels = path => {
      const page = open(path);
      return ['en', 'ko'].map(lang => page.get(`(() => {
        const kinds = {};
        for (const [id, say] of Object.entries(STR.${lang}.marks.kinds)) {
          kinds[id] = typeof say === 'function' ? say(3) : say;
        }
        return { legend: STR.${lang}.marks.legend, kinds };
      })()`));
    };
    const [en, ko] = labels(demoPath);
    assert.deepEqual(labels(servePath), [en, ko], 'the two pages label marks differently');
    assert.equal(en.kinds['pronoun:dangling'], 'points at nothing in your code');
    assert.equal(ko.kinds['pronoun:dangling'], '가리킬 대상이 코드에 없음');
  },

  // A prompt that is not graded has no score to have been rescaled, so its
  // report has no Grounded row; a graded one keeps it.
  'ungraded-report': async () => {
    for (const path of [demoPath, servePath]) {
      for (const lang of ['en', 'ko']) {
        const page = open(path);
        page.run(`setLang('${lang}')`);
        const report = async score => {
          page.run(`lastScore = ${JSON.stringify(score)}`);
          page.$('download').click();
          return page.downloads.at(-1).text();
        };
        const rpt = page.get(`STR.${lang}.rpt`);
        page.$('p').value = 'continue';
        const ungraded = await report(FOLLOW_UP);
        assert.ok(!ungraded.includes(`| ${rpt.grounded} |`), `${path} ${lang}:\n${ungraded}`);
        page.$('p').value = 'fix the bug';
        const graded = await report(GRADED);
        assert.ok(graded.includes(`| ${rpt.grounded} | ${rpt.no} |`), `${path} ${lang}:\n${graded}`);
      }
    }
  },

  // The footer's command stays on one line on a phone.
  'footer-code': async () => {
    const css = readFileSync(demoPath, 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1];
    assert.match(css, /footer code\s*\{[^}]*white-space:\s*nowrap/);
    const page = open(demoPath);
    for (const lang of ['en', 'ko']) {
      page.run(`setLang('${lang}')`);
      assert.match(page.$('foot').innerHTML, /<code>yp serve --repo \.<\/code>/, lang);
    }
  },

  // yp serve's report says the server scored it, not the browser.
  'served-report': async () => {
    for (const lang of ['en', 'ko']) {
      const page = open(servePath);
      page.run(`setLang('${lang}')`);
      page.$('p').value = 'fix the bug';
      page.run(`lastScore = ${JSON.stringify(GRADED)}`);
      page.$('download').click();
      const report = await page.downloads.at(-1).text();
      assert.match(report, /`yp serve`/, lang);
      assert.doesNotMatch(report, /in the browser|브라우저/, lang);
    }
  },
};

for (const name of only ? [only] : Object.keys(checks)) {
  if (!checks[name]) throw new Error(`no page check named ${name}`);
  await checks[name]();
  console.log(`ok - ${name}`);
}
