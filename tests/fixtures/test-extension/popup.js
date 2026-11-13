(async () => {
  const log = (k, v) => fetch('http://127.0.0.1:8765/log?' + k + '=' + encodeURIComponent(v)).catch(() => {});
  const s = await chrome.storage.local.get({ n: 0 });
  document.getElementById('o').textContent = 'n=' + s.n;
  const r = await chrome.runtime.sendMessage({ kind: 'popup' });
  document.getElementById('t').textContent = 'Popup ' + JSON.stringify(r);
  log('popup', JSON.stringify(r) + ' n=' + s.n);
})();
