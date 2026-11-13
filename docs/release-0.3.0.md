# Brújula 0.3.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**, como la 0.2.0.

## Novedades

- **Bienvenida en el primer arranque**: trae tus marcadores de Chrome, Chromium, Brave, Edge, Vivaldi, Opera, Firefox, Zen, LibreWolf o Floorp (o de un archivo HTML/JSON), te ofrece hacer Brújula tu navegador por defecto y elegir esenciales. En español, inglés, francés, alemán y portugués.
- **Perfiles**: cada persona con sus marcadores, historial, contraseñas y extensiones. Pantalla «¿Quién navega?», contraseña opcional (solo protege el selector, no cifra los datos) y Preferencias → Perfiles.
- **Esenciales** con el icono real de cada app (el del tema de iconos, o el favicon del sitio).
- **Panel de descargas**, carpetas de marcadores anidadas y nueva mascota.
- **Botón de página** en la barra de arriba (también en modo vertical): aparece cuando estás en una ficha de la Chrome Web Store («Añadir esta extensión») o en un sitio que declara ser una aplicación web («Instalar este sitio como aplicación web»).
- **Extensiones desde la Chrome Web Store**: abre la ficha de la extensión y pulsa el botón; Brújula descarga el paquete y te enseña los permisos antes de instalarlo.
- Arreglo: en Preferencias → Marcadores, los títulos y direcciones largos ya no se salen de la tarjeta.

## Conocido

- El botón «Añadir a Chrome» de la propia página de la Web Store puede no aparecer o no funcionar en Brújula (la tienda lo reserva a Chrome): usa el botón de Brújula.
- Importar: probado con un archivo de marcadores de Chromium; Firefox y los suyos leen el último respaldo de marcadores del perfil y aún no se han probado con datos reales.
- La contraseña de un perfil solo protege el selector, no cifra el disco. El AppImage necesita una glibc tan nueva como la de Arch.
- Extensiones: sin `webRequest`, menús contextuales, cookies ni mensajería nativa (ver `docs/brujula-rust.md`).

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.3.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
