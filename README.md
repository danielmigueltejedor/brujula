# Brújula

El navegador de [Nodalix](https://github.com/danielmigueltejedor/nodalix-os): WebKitGTK 6, interfaz con
el Nodalia Design Language (`nodalia-gtk`), pestañas verticales o horizontales, espacios, un bloqueador de
anuncios propio y compatibilidad con extensiones de Chromium. Escrito en Rust.

Solo Linux (GTK 4.20 o más y WebKitGTK 6.0).

## Qué trae

- **Rápido y ligero**: WebKit, pestañas que se duermen solas, la sesión se carga bajo demanda.
- **Seguro**: HTTPS primero, certificados malos nunca se saltan, descargas ejecutables bloqueadas, permisos denegados por defecto, ITP activado.
- **Bloqueador de anuncios** con listas EasyList, EasyPrivacy y uBlock (reglas de red de WebKit, reglas cosméticas y scriptlets), activable por sitio.
- **Personalizable**: tema, acento, marco con degradados, redondeo, separación, disposición (islas, marco, vertical, horizontal), espacios con icono, Boosts (CSS por sitio), buscador a elegir.
- **Extensiones de Chromium**: scripts de contenido, fondo, ventana emergente, opciones, almacenamiento, mensajes, reglas de bloqueo… Se instalan desde una carpeta, un `.crx` o la Chrome Web Store, con un aviso de permisos. No todas las extensiones funcionarán: ver [docs/brujula-rust.md](docs/brujula-rust.md).
- **Sincronización** de marcadores, espacios y pestañas por una carpeta (Dropbox, Nextcloud, iCloud Drive…) o una cuenta de Nodalix.
- **Contraseñas** guardadas cifradas (Argon2id + XChaCha20-Poly1305) con una contraseña maestra, que solo se rellenan en el sitio exacto y al tocar el campo.
- **Historial** local, con búsqueda en el Spotlight y borrado por horas o completo, y **ventana privada** (Ctrl+Mayús+N o `brujula --private`) que no guarda historial, cookies ni pestañas.
- Arc-isms: archivo automático, vista dividida, Peek (Mayús+clic), notas por espacio, ventana flotante.

## Instalar

En Arch o CachyOS:

```sh
cd packaging/arch && makepkg -si
```

Desde el código, con las dependencias (`gtk4`, `webkitgtk-6.0`, `libsoup3`) y Rust 1.85 o más:

```sh
cargo install --path crates/brujula --root ~/.local --locked
```

## Desarrollar

`docs/brujula-rust.md` explica la estructura, las decisiones, cómo compilar y probar y qué está
verificado y qué no. `crates/brujula-core` no tiene interfaz y se prueba con `cargo test -p brujula-core`.

## Licencia

GPL-3.0-or-later. El repositorio conserva el código de GNOME Web (Epiphany), de donde partió.
