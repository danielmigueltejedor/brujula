# Brújula 0.3.1

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Arreglos

- **Descargas de programas y scripts**: Brújula se negaba a bajar cualquier `.AppImage`, `.sh`, `.deb`… con un aviso. Ahora pregunta «¿Descargar este archivo?» y, si dices que sí, lo guarda en Descargas **sin permiso de ejecución** (dáselo tú con `chmod +x` si te fías). Si dices que no, no se guarda nada. Lo mismo en las aplicaciones web.
- Las descargas normales (documentos, imágenes, vídeos…) no cambian.

## Conocido

- Los diálogos de Brújula aún no se han revisado en un escritorio real tras este cambio: si «¿Descargar este archivo?» se ve mal, dilo.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.3.1-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
