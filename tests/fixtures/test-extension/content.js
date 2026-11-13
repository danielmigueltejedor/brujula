(async () => {
  const log = (k, v) => {
    fetch('http://127.0.0.1:8765/log?' + k + '=' + encodeURIComponent(v)).catch(() => {});
  };
  document.title = document.title + ' [ext:' + chrome.i18n.getMessage('hello', ['mundo']) + ']';
  try {
    const r = await chrome.runtime.sendMessage({ kind: 'hi', from: 'content' });
    document.getElementById('out').textContent += ' reply=' + JSON.stringify(r);
    log('content_reply', JSON.stringify(r));
  } catch (e) { log('content_error', String(e)); }
  const port = chrome.runtime.connect({ name: 'cs-port' });
  port.onMessage.addListener((m) => { log('content_port_msg', JSON.stringify(m)); port.disconnect(); });
  port.postMessage({ hello: 'port' });
  chrome.runtime.onMessage.addListener((m, sender, send) => {
    log('content_got', JSON.stringify(m));
    send({ pong: true });
  });
  log('content_css', getComputedStyle(document.body).borderTopWidth + ' h1=' + getComputedStyle(document.querySelector('h1')).color + ' sheets=' + document.styleSheets.length);
  const s = await chrome.storage.local.get({ n: 0 });
  log('content_storage', JSON.stringify(s));
})().catch((e) => console.error(e));
