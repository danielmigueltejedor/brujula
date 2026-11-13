// Brújula's passkey support, the half that lives in the page. WebKitGTK has no WebAuthn, so this
// gives the page `PublicKeyCredential` and `navigator.credentials` and hands each request to the
// browser, which asks a phone through a QR code. It is the only part the page can see, so it is
// as small as it can be and decides nothing: which site is asking is taken by the browser from
// the tab itself, never from here, and the answer is only ever a credential made by the phone.
// The other half (passkeys.js) runs in a world of its own and is the only one that talks to the
// browser.
(() => {
  'use strict';
  if (window.PublicKeyCredential || !window.isSecureContext || window.top !== window) return;

  const REQ = 'brujula-wa';
  const REPLY = 'brujula-wa-reply';
  const CANCEL = 'brujula-wa-cancel';

  // base64url <-> bytes
  const enc = (buf) => {
    const b = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
    let s = '';
    for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
    return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  };
  const dec = (s) => {
    const t = String(s).replace(/-/g, '+').replace(/_/g, '/');
    const bin = atob(t + '='.repeat((4 - (t.length % 4)) % 4));
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out.buffer;
  };
  const isBuf = (v) => v instanceof ArrayBuffer || ArrayBuffer.isView(v);
  // Options to JSON: every buffer becomes base64url, as in the WebAuthn JSON forms.
  const toJson = (v) => {
    if (isBuf(v)) return enc(v);
    if (Array.isArray(v)) return v.map(toJson);
    if (v && typeof v === 'object') {
      const o = {};
      for (const k of Object.keys(v)) if (v[k] !== undefined) o[k] = toJson(v[k]);
      return o;
    }
    return v;
  };
  // Extension outputs: these strings are buffers.
  const BUFFERS = new Set(['first', 'second', 'blob', 'output1', 'output2']);
  const outputs = (v) => {
    if (Array.isArray(v)) return v.map(outputs);
    if (v && typeof v === 'object') {
      const o = {};
      for (const k of Object.keys(v)) o[k] = BUFFERS.has(k) && typeof v[k] === 'string' ? dec(v[k]) : outputs(v[k]);
      return o;
    }
    return v;
  };

  const fail = (name, message) =>
    name === 'TypeError' ? new TypeError(message) : new DOMException(message, name);

  class Credential {
    get id() { return this._id; }
    get type() { return this._type; }
    static async isConditionalMediationAvailable() { return false; }
  }
  class AuthenticatorResponse {
    get clientDataJSON() { return this._clientDataJSON; }
  }
  class AuthenticatorAttestationResponse extends AuthenticatorResponse {
    get attestationObject() { return this._attestationObject; }
    getTransports() { return this._transports.slice(); }
    getAuthenticatorData() { return this._authenticatorData; }
    getPublicKey() { return this._publicKey; }
    getPublicKeyAlgorithm() { return this._publicKeyAlgorithm; }
  }
  class AuthenticatorAssertionResponse extends AuthenticatorResponse {
    get authenticatorData() { return this._authenticatorData; }
    get signature() { return this._signature; }
    get userHandle() { return this._userHandle; }
  }
  class PublicKeyCredential extends Credential {
    get rawId() { return this._rawId; }
    get response() { return this._response; }
    get authenticatorAttachment() { return this._attachment; }
    getClientExtensionResults() { return this._results; }
    toJSON() { return this._json; }
    static async isUserVerifyingPlatformAuthenticatorAvailable() { return false; }
    static async isConditionalMediationAvailable() { return false; }
    static async getClientCapabilities() {
      return {
        hybridTransport: true,
        passkeyPlatformAuthenticator: false,
        userVerifyingPlatformAuthenticator: false,
        conditionalCreate: false,
        conditionalGet: false,
        relatedOrigins: false,
        signalAllAcceptedCredentials: false,
        signalCurrentUserDetails: false,
        signalUnknownCredential: false,
      };
    }
    static parseCreationOptionsFromJSON(j) {
      const o = JSON.parse(JSON.stringify(j));
      o.challenge = dec(o.challenge);
      o.user.id = dec(o.user.id);
      for (const c of o.excludeCredentials || []) c.id = dec(c.id);
      return o;
    }
    static parseRequestOptionsFromJSON(j) {
      const o = JSON.parse(JSON.stringify(j));
      o.challenge = dec(o.challenge);
      for (const c of o.allowCredentials || []) c.id = dec(c.id);
      return o;
    }
  }
  for (const [c, n] of [
    [Credential, 'Credential'],
    [AuthenticatorResponse, 'AuthenticatorResponse'],
    [AuthenticatorAttestationResponse, 'AuthenticatorAttestationResponse'],
    [AuthenticatorAssertionResponse, 'AuthenticatorAssertionResponse'],
    [PublicKeyCredential, 'PublicKeyCredential'],
  ]) {
    Object.defineProperty(c.prototype, Symbol.toStringTag, { value: n, configurable: true });
  }

  // The credential the phone made, as an object the page can use.
  const credential = (j, kind) => {
    const r = j.response;
    const resp =
      kind === 'create' ? Object.create(AuthenticatorAttestationResponse.prototype) : Object.create(AuthenticatorAssertionResponse.prototype);
    resp._clientDataJSON = dec(r.clientDataJSON);
    if (kind === 'create') {
      resp._attestationObject = dec(r.attestationObject);
      resp._authenticatorData = dec(r.authenticatorData);
      resp._transports = Array.isArray(r.transports) ? r.transports : [];
      resp._publicKey = r.publicKey ? dec(r.publicKey) : null;
      resp._publicKeyAlgorithm = r.publicKeyAlgorithm;
    } else {
      resp._authenticatorData = dec(r.authenticatorData);
      resp._signature = dec(r.signature);
      resp._userHandle = r.userHandle ? dec(r.userHandle) : null;
    }
    const c = Object.create(PublicKeyCredential.prototype);
    c._id = j.id;
    c._type = 'public-key';
    c._rawId = dec(j.rawId);
    c._response = resp;
    c._attachment = j.authenticatorAttachment || 'cross-platform';
    c._results = outputs(j.clientExtensionResults || {});
    c._json = j;
    return c;
  };

  const pending = new Map();
  document.addEventListener(REPLY, (e) => {
    let m;
    try { m = JSON.parse(e.detail); } catch (_) { return; }
    const p = pending.get(m && m.id);
    if (!p) return;
    pending.delete(m.id);
    if (p.off) p.off();
    if (m.ok) {
      try { p.resolve(credential(JSON.parse(m.ok), p.kind)); } catch (_) { p.reject(fail('NotAllowedError', 'The passkey could not be read.')); }
    } else {
      p.reject(fail(m.err || 'NotAllowedError', m.msg || 'The operation was not allowed.'));
    }
  });

  let counter = 0;
  const newId = () => {
    const b = new Uint8Array(12);
    crypto.getRandomValues(b);
    return 'w' + (counter++).toString(36) + Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
  };

  const ask = (kind, pk, signal) =>
    new Promise((resolve, reject) => {
      if (signal && signal.aborted) return reject(signal.reason || fail('AbortError', 'The operation was aborted.'));
      let json;
      try {
        json = JSON.stringify(toJson(pk));
      } catch (_) {
        return reject(new TypeError('The options are not valid.'));
      }
      const id = newId();
      const p = { resolve, reject, kind };
      if (signal) {
        const onAbort = () => {
          if (!pending.delete(id)) return;
          document.dispatchEvent(new CustomEvent(CANCEL, { detail: id }));
          reject(signal.reason || fail('AbortError', 'The operation was aborted.'));
        };
        signal.addEventListener('abort', onAbort, { once: true });
        p.off = () => signal.removeEventListener('abort', onAbort);
      }
      pending.set(id, p);
      document.dispatchEvent(new CustomEvent(REQ, { detail: JSON.stringify({ id, kind, options: json }) }));
    });

  class CredentialsContainer {
    async create(options) {
      if (!options || !options.publicKey) {
        if (options && (options.password || options.federated)) throw fail('NotSupportedError', 'Only passkeys can be created.');
        throw new TypeError('The options must have a publicKey.');
      }
      if (options.mediation === 'conditional') throw fail('NotSupportedError', 'Conditional creation is not supported.');
      return ask('create', options.publicKey, options.signal);
    }
    async get(options) {
      if (!options || !options.publicKey) return null;
      if (options.mediation === 'conditional') throw fail('NotSupportedError', 'Conditional mediation is not supported.');
      if (options.mediation === 'silent') throw fail('NotAllowedError', 'A passkey is never given without asking.');
      return ask('get', options.publicKey, options.signal);
    }
    async store() { return undefined; }
    async preventSilentAccess() { return undefined; }
  }
  Object.defineProperty(CredentialsContainer.prototype, Symbol.toStringTag, { value: 'CredentialsContainer', configurable: true });

  const container = new CredentialsContainer();
  const define = (name, value) =>
    Object.defineProperty(window, name, { value, writable: true, configurable: true, enumerable: false });
  define('Credential', Credential);
  define('PublicKeyCredential', PublicKeyCredential);
  define('AuthenticatorResponse', AuthenticatorResponse);
  define('AuthenticatorAttestationResponse', AuthenticatorAttestationResponse);
  define('AuthenticatorAssertionResponse', AuthenticatorAssertionResponse);
  define('CredentialsContainer', CredentialsContainer);
  Object.defineProperty(Navigator.prototype, 'credentials', {
    get() { return container; },
    configurable: true,
    enumerable: true,
  });
})();
