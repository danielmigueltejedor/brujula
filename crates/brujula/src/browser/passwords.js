// Brújula's password helper. It runs in a world of its own, in the top frame only: the page
// cannot see this code or the message handler it talks to. The only things sent to the browser
// are what was typed when a login form is sent, and requests that come from a real click or key
// press (event.isTrusted), so a page cannot make the browser hand out a password by itself.
(() => {
  if (window.__brujulaPw) return;
  window.__brujulaPw = true;
  const handler = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.brujulaPw;
  if (!handler) return;
  const ask = (o) =>
    handler.postMessage(JSON.stringify(o)).then(
      (r) => {
        try { return JSON.parse(r); } catch (_) { return null; }
      },
      () => null
    );
  const shown = (el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  const usable = (el) => !el.disabled && !el.readOnly && shown(el);
  const textual = (el) => /^(text|email|tel|)$/i.test(el.getAttribute('type') || '');
  const scope = (el) => el.form || el.closest('form') || document.body;
  const passwords = (root) => [...root.querySelectorAll('input[type=password]')].filter(usable);

  // The user name field that goes with a password field: the closest text field before it.
  const userFor = (pw) => {
    const root = scope(pw);
    const fields = [...root.querySelectorAll('input')].filter((i) => textual(i) && usable(i));
    let best = null;
    for (const f of fields) {
      if (f.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING) best = f;
      if (f.autocomplete === 'username') return f;
    }
    return best;
  };
  // Which password field a field the user touched belongs to.
  const pairOf = (el) => {
    if (!(el instanceof HTMLInputElement)) return null;
    if (el.type === 'password') return usable(el) ? { pw: el, user: userFor(el) } : null;
    if (!textual(el) || !usable(el)) return null;
    const pws = passwords(scope(el)).filter((p) => el.compareDocumentPosition(p) & Node.DOCUMENT_POSITION_FOLLOWING);
    return pws.length === 1 ? { pw: pws[0], user: el } : null;
  };
  const setValue = (el, v) => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const fill = async (pair, id) => {
    const r = await ask({ t: 'fill', href: location.href, id });
    if (!r || typeof r.p !== 'string') return;
    if (pair.user && !pair.user.value) setValue(pair.user, r.u);
    setValue(pair.pw, r.p);
  };

  let menu = null;
  const closeMenu = () => {
    if (menu) { menu.remove(); menu = null; }
  };
  const chooser = (pair, logins) => {
    closeMenu();
    const host = document.createElement('div');
    const root = host.attachShadow({ mode: 'closed' });
    const box = document.createElement('div');
    box.style.cssText = 'all:initial;position:fixed;z-index:2147483647;background:#fff;color:#111;border:1px solid #bbb;border-radius:8px;box-shadow:0 4px 18px #0004;font:14px sans-serif;padding:4px;min-width:200px';
    for (const l of logins) {
      const b = document.createElement('div');
      b.textContent = l.u || '(sin usuario)';
      b.style.cssText = 'padding:7px 10px;border-radius:6px;cursor:pointer';
      b.onmouseenter = () => (b.style.background = '#e8f5ee');
      b.onmouseleave = () => (b.style.background = '');
      b.addEventListener('mousedown', (e) => {
        if (!e.isTrusted) return;
        e.preventDefault();
        closeMenu();
        fill(pair, l.id);
      });
      box.appendChild(b);
    }
    const r = pair.pw.getBoundingClientRect();
    box.style.left = Math.max(4, r.left) + 'px';
    box.style.top = Math.min(innerHeight - 8, r.bottom + 4) + 'px';
    root.appendChild(box);
    document.documentElement.appendChild(host);
    menu = host;
  };

  const done = new WeakSet();
  const offer = async (el) => {
    const pair = pairOf(el);
    if (!pair || done.has(pair.pw)) return;
    done.add(pair.pw);
    const r = await ask({ t: 'list', href: location.href });
    if (!r || !r.logins || !r.logins.length) { done.delete(pair.pw); return; }
    const typed = pair.user && pair.user.value;
    const mine = typed && r.logins.find((l) => l.u === typed);
    if (mine) fill(pair, mine.id);
    else if (r.logins.length === 1) fill(pair, r.logins[0].id);
    else chooser(pair, r.logins);
  };
  const touched = (e) => {
    if (!e.isTrusted) return;
    const t = e.target;
    if (menu && !(t instanceof HTMLInputElement)) closeMenu();
    if (e.type === 'keydown' && (e.key === 'Tab' || e.key === 'Escape')) { closeMenu(); return; }
    if (t instanceof HTMLInputElement) offer(t);
  };
  document.addEventListener('pointerdown', touched, true);
  document.addEventListener('keydown', touched, true);

  // What was typed when a login form is sent.
  let last = '';
  let lastAt = 0;
  const report = (root) => {
    const pws = passwords(root).filter((p) => p.value);
    if (!pws.length) return;
    const news = pws.filter((p) => p.autocomplete === 'new-password');
    const pw = news[0] || (pws.length >= 3 ? pws[1] : pws[0]);
    const user = userFor(pw);
    const u = user ? user.value.trim() : '';
    const key = u + '\u0000' + pw.value;
    const now = Date.now();
    if (key === last && now - lastAt < 1500) return;
    last = key;
    lastAt = now;
    ask({ t: 'submit', href: location.href, u, p: pw.value });
  };
  document.addEventListener('submit', (e) => { if (e.isTrusted || e.target.tagName === 'FORM') report(e.target); }, true);
  document.addEventListener('click', (e) => {
    if (!e.isTrusted) return;
    const b = e.target.closest && e.target.closest('button, input[type=submit], [role=button]');
    if (!b) return;
    let root = b.form || b.closest('form');
    for (let i = 0, n = b.parentElement; !root && n && i < 4; i++, n = n.parentElement) {
      if (passwords(n).length) root = n;
    }
    if (root) report(root);
  }, true);
  document.addEventListener('keydown', (e) => {
    if (!e.isTrusted || e.key !== 'Enter') return;
    const t = e.target;
    if (t instanceof HTMLInputElement && t.type === 'password') report(scope(t));
  }, true);
})();
