// Brújula's passkey support, the half that talks to the browser. It runs in a world of its own,
// in the top frame only: the page cannot see this code or the message handler it uses. It takes
// the request the page script (passkeys_main.js) announces and gives it to the browser, which
// asks the browser's own tab, not this message, who is asking, so a page cannot ask in another
// site's name. The options wait here until the browser comes for them from the tab that is in
// front; a tab in the background finds nothing and is turned down.
(() => {
  if (window.__brujulaWa) return;
  const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.brujulaWa;
  if (!handler) return;
  const MAX = 64 * 1024;
  const waiting = new Map();
  const reply = (id, o) =>
    document.dispatchEvent(new CustomEvent('brujula-wa-reply', { detail: JSON.stringify(Object.assign({ id }, o)) }));

  // The browser takes a request out of here once, when it checks who is asking.
  window.__brujulaWa = {
    take(id) {
      const w = waiting.get(id);
      waiting.delete(id);
      return w ? JSON.stringify({ kind: w.kind, options: w.options }) : null;
    },
  };

  document.addEventListener('brujula-wa', (e) => {
    let m;
    try { m = JSON.parse(e.detail); } catch (_) { return; }
    if (!m || typeof m.id !== 'string' || !/^[a-z0-9]{8,80}$/.test(m.id)) return;
    const refuse = (name, msg) => reply(m.id, { err: name, msg });
    if (m.kind !== 'create' && m.kind !== 'get') return refuse('TypeError', 'Unknown request.');
    if (typeof m.options !== 'string' || m.options.length > MAX) return refuse('TypeError', 'The options are too large.');
    // The person has to have used the page: a page that was just opened cannot ask by itself.
    const ua = navigator.userActivation;
    if (ua && !ua.hasBeenActive) return refuse('NotAllowedError', 'The page has not been used yet.');
    waiting.set(m.id, { kind: m.kind, options: m.options });
    setTimeout(() => waiting.delete(m.id), 30000);
    handler.postMessage(JSON.stringify({ t: 'req', id: m.id })).then(
      (r) => {
        let a = null;
        try { a = JSON.parse(r); } catch (_) {}
        reply(m.id, a && (a.ok || a.err) ? a : { err: 'NotAllowedError', msg: 'The operation was not allowed.' });
      },
      () => reply(m.id, { err: 'NotAllowedError', msg: 'The operation was not allowed.' })
    );
  });

  document.addEventListener('brujula-wa-cancel', (e) => {
    if (typeof e.detail === 'string' && /^[a-z0-9]{8,80}$/.test(e.detail)) {
      waiting.delete(e.detail);
      handler.postMessage(JSON.stringify({ t: 'cancel', id: e.detail })).catch(() => {});
    }
  });
})();
