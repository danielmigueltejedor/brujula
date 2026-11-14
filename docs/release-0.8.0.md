# Brújula 0.8.0 (borrador)

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

Borrador de las notas: la versión del `Cargo.toml` sigue en 0.7.0 hasta que se decida la release.
Nada de esto se ha visto en pantalla (ver la PR #41).

## Novedades

- **Buscar en la página** (Ctrl+F): una barra sobre la página con el número de resultados, flechas (Intro / Mayús+Intro) y Esc para cerrar.
- **Zoom que se recuerda por sitio** (Ctrl +, Ctrl −, Ctrl 0): cada sitio vuelve a abrirse con el zoom que le diste, salvo en ventanas privadas.
- **Silenciar una pestaña**: clic en el altavoz de la pestaña, Ctrl+M, o el menú de la pestaña.
- **Copiar el enlace de la página** (Ctrl+Mayús+C) y **capturar la página** (Ctrl+Mayús+S: se guarda en Imágenes y se copia al portapapeles).
- **Limpiar pestañas sin anclar** (Ctrl+Mayús+K): archiva de golpe las pestañas del espacio que no están ancladas, en una carpeta ni delante, con un aviso para deshacer.
- **Comandos en el Spotlight**: escribe «capt», «zoom», «imprimir», «limpiar»… y salen los comandos, con su atajo.
- **Más atajos**: imprimir (Ctrl+P), pantalla completa (F11), inspector (F12 o Ctrl+Mayús+I) y Ctrl+1…9 para ir a una pestaña del espacio. Todo está también en el menú → Página.
- **Contenido con DRM** (Netflix, Disney+, Prime, Spotify…): WebKitGTK no puede reproducirlo, así que Brújula avisa y abre la página en un navegador con Widevine (Chrome, Edge, Firefox…). Se configura en Preferencias → Seguridad. Ver `docs/drm.md`.
- **Movimiento**: tarjetas que suben y se aclaran al abrirse (Spotlight, Peek, avisos, buscar), pestañas nuevas que entran desde el lado, fundidos suaves y esenciales y espacios que se elevan al pasar el ratón. Se apaga solo si el escritorio pide sin animaciones.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.8.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
