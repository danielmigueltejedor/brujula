const log = (k, v) => {
  fetch('http://127.0.0.1:8765/log?' + k + '=' + encodeURIComponent(v)).catch(() => {});
};
log('bg_started', chrome.runtime.getManifest().name);
chrome.runtime.onInstalled.addListener((d) => log('bg_installed', d.reason));
chrome.runtime.onMessage.addListener((m, sender, send) => {
  if (m && m.kind === 'hi') {
    chrome.storage.local.get({ n: 0 }).then(async (s) => {
      await chrome.storage.local.set({ n: s.n + 1 });
      log('bg_hi', 'tab=' + (sender.tab && sender.tab.id) + ' url=' + (sender.tab && sender.tab.url));
      const tabs = await chrome.tabs.query({ active: true });
      log('bg_tabs', tabs.length + ':' + (tabs[0] && tabs[0].url));
      send({ ok: true, n: s.n + 1 });
      // talk back to the tab
      if (sender.tab) {
        const r = await chrome.tabs.sendMessage(sender.tab.id, { kind: 'hello-tab' });
        log('bg_tab_reply', JSON.stringify(r));
        const x = await chrome.scripting.executeScript({ target: { tabId: sender.tab.id }, func: (a) => document.title + a, args: ['!'] });
        log('bg_script', JSON.stringify(x));
      }
    }).catch((e) => log('bg_error', String(e)));
    return true;
  }
  if (m && m.kind === 'popup') { send({ fromBg: 'bien' }); }
});
chrome.storage.onChanged.addListener((c, area) => log('bg_changed', area + JSON.stringify(c)));

chrome.runtime.onConnect.addListener((port) => {
  log('bg_port_open', port.name + ' from ' + (port.sender && port.sender.tab ? 'tab' : 'ext'));
  port.onMessage.addListener((m) => { log('bg_port_msg', JSON.stringify(m)); port.postMessage({ echo: m }); });
  port.onDisconnect.addListener(() => log('bg_port_closed', port.name));
});
