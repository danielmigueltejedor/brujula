# Brújula 0.1.0

Primera versión. Solo Linux (GTK 4.20 o más, WebKitGTK 6.0).

## Novedades

- Navegador en Rust con WebKitGTK 6 y el Nodalia Design Language: pestañas verticales u horizontales, espacios con icono, marcadores, Spotlight, borde inteligente.
- Bloqueador de anuncios propio (EasyList, EasyPrivacy, uBlock): reglas de red de WebKit, reglas cosméticas y scriptlets; se apaga por sitio con el escudo.
- **Extensiones de Chromium**: instala desde una carpeta, un `.crx` o la Chrome Web Store, con aviso de permisos. Scripts y CSS de contenido, página de fondo, ventana emergente, opciones, `chrome.storage`, mensajes y puertos, `chrome.tabs`, `chrome.scripting`, `fetch` con permisos de host y reglas `declarativeNetRequest` estáticas. No todas funcionarán.
- Seguridad: HTTPS primero, certificados malos sin excepción, descargas ejecutables bloqueadas, permisos denegados, ITP.
- Personalización: tema, acento, marco con degradados, radio, separación, transparencia, disposición, buscador, Boosts (CSS por sitio).
- Sincronización por carpeta (Dropbox, Nextcloud, iCloud Drive…) o cuenta de Nodalix; archivo automático de pestañas; pestañas dormidas; vista dividida; Peek; notas; ventana flotante.

## Conocido

- Sin gestor de contraseñas, historial ni modo privado.
- Extensiones: sin `webRequest`, menús contextuales, cookies ni mensajería nativa (ver `docs/brujula-rust.md`).
- La descarga real de las listas del bloqueador y la instalación desde la Chrome Web Store se probaron solo hasta el cliente HTTP.

## Instalar

Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
