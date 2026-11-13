# Brújula 0.2.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Esta versión trae **AppImage**.

## Novedades

- **Contraseñas**: una bóveda cifrada con tu contraseña maestra (Argon2id y XChaCha20-Poly1305, archivo solo legible por ti). Brújula pregunta antes de guardar un inicio de sesión y solo lo rellena en el sitio exacto donde se guardó, y solo cuando tocas el campo. Se bloquea sola a los 10 minutos. Preferencias → Contraseñas. No se sincronizan.
- **Aplicaciones web**: menú → «Instalar como aplicación web…» (o Preferencias → Aplicaciones web) y el sitio se abre en su propia ventana, con su icono en el lanzador y su propio perfil. Con bloqueador; los enlaces de otros sitios se abren en tu navegador.
- **Historial** local, con búsqueda en el Spotlight y borrado por horas o completo.
- **Ventana privada** (Ctrl+Mayús+N o `brujula --private`): no guarda historial, cookies, pestañas ni contraseñas.
- **AppImage** para x86_64, con su `SHA256SUMS`, adjunta a la release.

## Conocido

- El AppImage se construye en Arch: necesita una distribución con una glibc igual de nueva (Arch, CachyOS, Nodalix y parecidas).
- Contraseñas: no se rellenan formularios dentro de iframes ni algunas aplicaciones de una sola página que no usan el envío normal del formulario. Aún no se importan de otros navegadores.
- Aplicaciones web: las ventanas emergentes de inicio de sesión no se abren; la cámara y el micrófono piden permiso pero no se han probado en una videollamada real.
- Extensiones: sin `webRequest`, menús contextuales, cookies ni mensajería nativa (ver `docs/brujula-rust.md`).

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.2.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
