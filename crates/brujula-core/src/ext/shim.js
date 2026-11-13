// The `chrome.*` API of an extension, over the browser's own message channel. Injected before
// the extension's scripts, with `__brujulaCtx` set to {id, kind, manifest, messages, base, tab}:
// kind is "content" (a content script), "background", "popup" or "page".
(function () {
  'use strict';
  const ctx = globalThis.__brujulaCtx;
  if (!ctx || globalThis.__brujulaExt) return;
  const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers[ctx.handler];
  const isContent = ctx.kind === 'content';

  // ---- the channel to the browser: {ok, value} or {ok: false, error}
  async function call(op, args) {
    if (!handler) throw new Error('El navegador no responde');
    const reply = await handler.postMessage({ op, args: args === undefined ? null : args, href: location.href, tab: ctx.tab });
    if (reply && reply.ok === false) throw new Error(reply.error || 'error');
    return reply ? reply.value : undefined;
  }

  const runtime = { lastError: null };
  // Callback style and promise style both work: the last argument decides.
  function dual(op, prep) {
    return function (...args) {
      let cb = null;
      if (args.length && typeof args[args.length - 1] === 'function') cb = args.pop();
      const p = call(op, prep ? prep(...args) : args);
      if (!cb) return p;
      p.then(
        (v) => { runtime.lastError = null; cb(v); },
        (e) => { runtime.lastError = { message: String(e && e.message || e) }; try { cb(); } finally { runtime.lastError = null; } }
      );
    };
  }

  function makeEvent() {
    const ls = new Set();
    return {
      addListener(f) { if (typeof f === 'function') ls.add(f); },
      removeListener(f) { ls.delete(f); },
      hasListener(f) { return ls.has(f); },
      hasListeners() { return ls.size > 0; },
      _fire(...a) { for (const f of [...ls]) { try { f(...a); } catch (e) { console.error(e); } } },
      _ls: ls,
    };
  }

  // ---- messages from the browser to this context
  const onMessage = makeEvent();
  function senderOf(s) { return s || {}; }
  globalThis.__brujulaDeliver = function (reqId, msg, sender) {
    let done = false;
    const respond = (value) => {
      if (done) return;
      done = true;
      call('deliver.response', { reqId, value: value === undefined ? null : value }).catch(() => {});
    };
    let async = false;
    for (const f of [...onMessage._ls]) {
      try {
        const r = f(msg, senderOf(sender), (v) => respond(v));
        if (r === true) async = true;
        else if (r && typeof r.then === 'function') { async = true; r.then(respond, () => respond(null)); }
      } catch (e) { console.error(e); }
    }
    if (!async && !done) respond(null);
  };

  // ---- chrome.runtime
  const events = {};
  const ev = (name) => (events[name] || (events[name] = makeEvent()));
  Object.assign(runtime, {
    id: ctx.id,
    getURL: (p) => ctx.base + String(p).replace(/^\/+/, ''),
    getManifest: () => JSON.parse(JSON.stringify(ctx.manifest)),
    sendMessage: dual('runtime.sendMessage', (...a) => {
      // sendMessage([extensionId], message, [options], [callback])
      const msg = typeof a[0] === 'string' && a.length > 1 ? a[1] : a[0];
      return { msg };
    }),
    getPlatformInfo: dual('runtime.getPlatformInfo'),
    openOptionsPage: dual('runtime.openOptionsPage'),
    reload: () => {},
    getBackgroundPage: (cb) => cb && cb(null),
    setUninstallURL: (u, cb) => cb && cb(),
    connect() { return makePort(); },
    onMessage,
    onMessageExternal: makeEvent(),
    onConnect: makeEvent(),
    onInstalled: ev('runtime.onInstalled'),
    onStartup: ev('runtime.onStartup'),
    onSuspend: makeEvent(),
    onUpdateAvailable: makeEvent(),
  });
  function makePort() {
    const port = { name: '', onMessage: makeEvent(), onDisconnect: makeEvent(), postMessage(m) { runtime.sendMessage(m, (r) => { if (r !== undefined && r !== null) port.onMessage._fire(r); }); }, disconnect() {} };
    return port;
  }

  // ---- chrome.storage
  function area(name) {
    return {
      get: dual('storage.get', (keys) => ({ area: name, keys: keys === undefined ? null : keys })),
      set: dual('storage.set', (items) => ({ area: name, items })),
      remove: dual('storage.remove', (keys) => ({ area: name, keys })),
      clear: dual('storage.clear', () => ({ area: name })),
      getBytesInUse: dual('storage.bytes', () => ({ area: name })),
      setAccessLevel: dual('storage.noop'),
      onChanged: ev('storage.' + name + '.onChanged'),
    };
  }
  const storage = { local: area('local'), sync: area('sync'), session: area('session'), managed: area('managed'), onChanged: ev('storage.onChanged') };
  storage.managed.set = () => Promise.reject(new Error('managed storage is read-only'));

  // ---- chrome.i18n
  const i18n = {
    getMessage(name, subs) {
      const m = ctx.messages && ctx.messages[String(name).toLowerCase()];
      if (!m) return '';
      const list = subs === undefined ? [] : Array.isArray(subs) ? subs : [subs];
      const sub = (c) => /^\$(\d)$/.test(c) ? String(list[Number(c[1]) - 1] ?? '') : c;
      return m.message.replace(/\$(\w+)\$|\$(\d)|\$\$/g, (all, ph, d) => {
        if (all === '$$') return '$';
        if (d) return String(list[Number(d) - 1] ?? '');
        const p = m.placeholders && m.placeholders[ph.toLowerCase()];
        return p === undefined ? '' : sub(p);
      });
    },
    getUILanguage: () => ctx.lang || 'en',
    getAcceptLanguages: (cb) => { const r = [ctx.lang || 'en']; return cb ? cb(r) : Promise.resolve(r); },
    detectLanguage: (t, cb) => { const r = { isReliable: false, languages: [] }; return cb ? cb(r) : Promise.resolve(r); },
  };

  // ---- chrome.tabs / windows / scripting
  const tabs = {
    query: dual('tabs.query', (q) => ({ query: q || {} })),
    get: dual('tabs.get', (id) => ({ id })),
    getCurrent: dual('tabs.getCurrent'),
    create: dual('tabs.create', (p) => ({ props: p || {} })),
    update: dual('tabs.update', (...a) => (typeof a[0] === 'number' ? { id: a[0], props: a[1] || {} } : { id: null, props: a[0] || {} })),
    remove: dual('tabs.remove', (ids) => ({ ids: Array.isArray(ids) ? ids : [ids] })),
    reload: dual('tabs.reload', (id) => ({ id: typeof id === 'number' ? id : null })),
    sendMessage: dual('tabs.sendMessage', (id, msg) => ({ id, msg })),
    executeScript: dual('scripting.executeScript', (...a) => (typeof a[0] === 'number' ? { id: a[0], details: a[1] || {} } : { id: null, details: a[0] || {} })),
    insertCSS: dual('scripting.insertCSS', (...a) => (typeof a[0] === 'number' ? { id: a[0], details: a[1] || {} } : { id: null, details: a[0] || {} })),
    onCreated: ev('tabs.onCreated'), onUpdated: ev('tabs.onUpdated'), onRemoved: ev('tabs.onRemoved'),
    onActivated: ev('tabs.onActivated'), onReplaced: makeEvent(), onMoved: makeEvent(), onHighlighted: makeEvent(),
    TAB_ID_NONE: -1,
  };
  const windows = {
    getCurrent: dual('windows.get'), getLastFocused: dual('windows.get'), get: dual('windows.get'),
    getAll: dual('windows.getAll'), create: dual('tabs.create', (p) => ({ props: p || {} })), update: dual('windows.get'),
    onFocusChanged: makeEvent(), onCreated: makeEvent(), onRemoved: makeEvent(),
    WINDOW_ID_NONE: -1, WINDOW_ID_CURRENT: -2,
  };
  const scripting = {
    executeScript: dual('scripting.executeScript', (d) => ({ id: d && d.target ? d.target.tabId : null, details: d || {}, mv3: true })),
    insertCSS: dual('scripting.insertCSS', (d) => ({ id: d && d.target ? d.target.tabId : null, details: d || {}, mv3: true })),
    removeCSS: dual('storage.noop'),
    registerContentScripts: dual('storage.noop'), unregisterContentScripts: dual('storage.noop'),
    getRegisteredContentScripts: (cb) => (cb ? cb([]) : Promise.resolve([])),
  };

  // ---- chrome.action (and browserAction)
  const action = {
    setBadgeText: dual('action.set', (d) => ({ badge: d && d.text !== undefined ? String(d.text) : '' })),
    setBadgeBackgroundColor: dual('action.set', (d) => ({ color: d && d.color })),
    setBadgeTextColor: dual('storage.noop'),
    setTitle: dual('action.set', (d) => ({ title: d && d.title })),
    setPopup: dual('action.set', (d) => ({ popup: d && d.popup })),
    setIcon: dual('storage.noop'),
    getBadgeText: dual('action.get', () => ({ what: 'badge' })),
    getTitle: dual('action.get', () => ({ what: 'title' })),
    getPopup: dual('action.get', () => ({ what: 'popup' })),
    enable: dual('storage.noop'), disable: dual('storage.noop'), openPopup: dual('action.openPopup'),
    onClicked: ev('action.onClicked'),
  };

  // ---- chrome.alarms (they only fire while the background page is alive, which is always)
  const alarms = (() => {
    const timers = new Map();
    const onAlarm = ev('alarms.onAlarm');
    const info = new Map();
    function create(...a) {
      const name = typeof a[0] === 'string' ? a[0] : '';
      const o = (typeof a[0] === 'string' ? a[1] : a[0]) || {};
      clear(name);
      const when = o.when !== undefined ? Math.max(0, o.when - Date.now()) : Math.max(0, (o.delayInMinutes ?? o.periodInMinutes ?? 0) * 60000);
      const period = o.periodInMinutes ? o.periodInMinutes * 60000 : 0;
      info.set(name, { name, scheduledTime: Date.now() + when, periodInMinutes: o.periodInMinutes });
      const fire = () => { onAlarm._fire({ name, scheduledTime: Date.now(), periodInMinutes: o.periodInMinutes }); };
      const t = setTimeout(() => {
        fire();
        if (period) timers.set(name, setInterval(fire, period)); else { timers.delete(name); info.delete(name); }
      }, when);
      timers.set(name, t);
    }
    function clear(name, cb) { const had = timers.has(name); clearTimeout(timers.get(name)); clearInterval(timers.get(name)); timers.delete(name); info.delete(name); return cb ? cb(had) : Promise.resolve(had); }
    return {
      create,
      get: (n, cb) => { const r = info.get(n); return cb ? cb(r) : Promise.resolve(r); },
      getAll: (cb) => { const r = [...info.values()]; return cb ? cb(r) : Promise.resolve(r); },
      clear, clearAll: (cb) => { for (const n of [...timers.keys()]) clear(n); return cb ? cb(true) : Promise.resolve(true); },
      onAlarm,
    };
  })();

  const permissions = {
    contains: (p, cb) => { const ok = true; return cb ? cb(ok) : Promise.resolve(ok); },
    getAll: (cb) => { const r = { permissions: ctx.manifest.permissions || [], origins: ctx.manifest.host_permissions || [] }; return cb ? cb(r) : Promise.resolve(r); },
    request: (p, cb) => (cb ? cb(false) : Promise.resolve(false)),
    remove: (p, cb) => (cb ? cb(false) : Promise.resolve(false)),
    onAdded: makeEvent(), onRemoved: makeEvent(),
  };
  const management = {
    getSelf: (cb) => { const r = { id: ctx.id, name: ctx.manifest.name, version: ctx.manifest.version, enabled: true, installType: 'normal', type: 'extension' }; return cb ? cb(r) : Promise.resolve(r); },
    getAll: (cb) => (cb ? cb([]) : Promise.resolve([])),
  };
  const declarativeNetRequest = {
    getDynamicRules: (cb) => (cb ? cb([]) : Promise.resolve([])),
    getSessionRules: (cb) => (cb ? cb([]) : Promise.resolve([])),
    updateDynamicRules: (o, cb) => (cb ? cb() : Promise.resolve()),
    updateSessionRules: (o, cb) => (cb ? cb() : Promise.resolve()),
    updateEnabledRulesets: (o, cb) => (cb ? cb() : Promise.resolve()),
    getEnabledRulesets: (cb) => (cb ? cb([]) : Promise.resolve([])),
    setExtensionActionOptions: (o, cb) => (cb ? cb() : Promise.resolve()),
    isRegexSupported: (o, cb) => (cb ? cb({ isSupported: false }) : Promise.resolve({ isSupported: false })),
    getMatchedRules: (o, cb) => (cb ? cb({ rulesMatchedInfo: [] }) : Promise.resolve({ rulesMatchedInfo: [] })),
    MAX_NUMBER_OF_STATIC_RULESETS: 50, MAX_NUMBER_OF_ENABLED_STATIC_RULESETS: 10, GUARANTEED_MINIMUM_STATIC_RULES: 30000,
  };

  // ---- everything else is accepted and does nothing, so that an extension that only uses it on
  // the side keeps working: events that never fire, calls that resolve with nothing.
  function stub(path) {
    const target = function () {};
    return new Proxy(target, {
      get(_, prop) {
        if (prop === 'then') return undefined;
        if (typeof prop === 'symbol') return undefined;
        const name = String(prop);
        if (/^on[A-Z]/.test(name)) return ev(path + '.' + name);
        if (/^[A-Z_0-9]+$/.test(name)) return name;
        return stub(path + '.' + name);
      },
      apply(_, __, args) {
        const cb = args.length && typeof args[args.length - 1] === 'function' ? args[args.length - 1] : null;
        return cb ? (cb(), undefined) : Promise.resolve(undefined);
      },
    });
  }

  const known = {
    runtime, storage, i18n, tabs, windows, scripting, action, browserAction: action, pageAction: action,
    alarms, permissions, management, declarativeNetRequest,
    extension: { getURL: runtime.getURL, getBackgroundPage: () => null, inIncognitoContext: false, onMessage, sendMessage: runtime.sendMessage },
  };
  const chromeObj = new Proxy(known, {
    get(t, prop) {
      if (prop in t) return t[prop];
      if (typeof prop === 'symbol') return undefined;
      return stub('chrome.' + String(prop));
    },
    has() { return true; },
  });
  Object.defineProperty(globalThis, '__brujulaExt', { value: { events, ctx }, enumerable: false });
  Object.defineProperty(globalThis, 'chrome', { value: chromeObj, configurable: true, writable: true });
  Object.defineProperty(globalThis, 'browser', { value: chromeObj, configurable: true, writable: true });

  // ---- events pushed by the browser: tab changes, storage changes, the install event
  globalThis.__brujulaEmit = function (name, args) {
    const e = events[name];
    if (e) e._fire(...(args || []));
    if (name === 'storage.onChanged') {
      const [changes, areaName] = args;
      const a = storage[areaName];
      if (a && a.onChanged) a.onChanged._fire(changes);
    }
  };

  // ---- cross-origin requests of the extension's own pages go through the browser, which checks
  // them against the hosts the extension asked for (a page of the extension is not bound by CORS)
  if (!isContent && typeof fetch === 'function') {
    const realFetch = fetch.bind(globalThis);
    const hosts = (ctx.manifest.host_permissions || []).concat((ctx.manifest.permissions || []).filter((p) => /:\/\/|<all_urls>/.test(p)));
    const toB64 = (buf) => { let s = ''; const b = new Uint8Array(buf); for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode.apply(null, b.subarray(i, i + 0x8000)); return btoa(s); };
    const fromB64 = (t) => { const s = atob(t); const b = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i); return b; };
    globalThis.fetch = async function (input, init) {
      const req = new Request(input, init);
      const u = new URL(req.url);
      if (!/^https?:$/.test(u.protocol) || !hosts.length) return realFetch(input, init);
      const body = ['GET', 'HEAD'].includes(req.method) ? null : toB64(await req.arrayBuffer());
      const headers = [...req.headers.entries()];
      const r = await call('net.fetch', { url: req.url, method: req.method, headers, body });
      const nobody = [101, 204, 205, 304].includes(r.status);
      return new Response(nobody ? null : fromB64(r.body), { status: r.status, statusText: r.statusText, headers: r.headers });
    };
  }

  // ---- a popup tells the browser how big it wants to be
  if (ctx.kind === 'popup') {
    const size = () => {
      const d = document.documentElement, b = document.body;
      call('ui.resize', { w: Math.ceil(Math.max(d.scrollWidth, b ? b.scrollWidth : 0)), h: Math.ceil(Math.max(d.scrollHeight, b ? b.scrollHeight : 0)) }).catch(() => {});
    };
    addEventListener('load', () => { size(); if (window.ResizeObserver) new ResizeObserver(size).observe(document.documentElement); });
  }
  // The window of a popup or page can be closed by the extension.
  if (ctx.kind === 'background') {
    setTimeout(() => ev('runtime.onStartup')._fire(), 0);
    if (ctx.fresh) setTimeout(() => ev('runtime.onInstalled')._fire({ reason: 'install' }), 0);
  }
})();
