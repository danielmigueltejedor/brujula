# Brújula 0.4.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Novedades

- **Actualizaciones dentro de Brújula**: el AppImage mira GitHub de vez en cuando, descarga la versión nueva, comprueba su suma SHA-256, sustituye al AppImage y se reinicia con tus pestañas. Una copia instalada por el sistema (en Nodalix, el paquete) solo te avisa y se actualiza con las actualizaciones del sistema. Se activa o desactiva en Preferencias → Acerca de.
- **Novedades tras actualizar**: la primera vez que abres Brújula después de actualizar, se abre una pestaña con lo nuevo de la versión.
- **Acerca de** en el menú, como el resto de aplicaciones de Nodalia.
- **Esenciales como pestaña propia**: pulsar un esencial lo abre sin crear una pestaña en la lista; su icono es su pestaña (punto cuando está abierto, resaltado cuando está delante). Un enlace de un esencial que lleva a otro sitio se ve en una tarjeta encima, sin pulsar Mayús.
- **Pestañas reordenables**: arrástralas por la barra lateral; una línea marca dónde caerán, y toman la carpeta de la pestaña sobre la que las sueltas.
- **Historial en la barra lateral**: un botón (o Ctrl+H) desliza el historial por encima de las pestañas, con buscador y días; con las pestañas arriba se abre como una lista desde la barra.
- **Texturas**: granulado, líneas, puntos y cuadrícula para el marco y los paneles, con intensidad (Preferencias → Apariencia).
- **Descargas seguras por defecto**: cada archivo se guarda con un nombre oculto, se analiza al llegar (sin frenar la descarga) y solo entonces recibe su nombre. Si dice ser un documento, una imagen o un vídeo y por dentro es un programa, o si ClamAV está instalado y lo reconoce, te pregunta si lo borras.
- **Tipografía de Nodalia Design**: Inter para el texto y Sora para los títulos, también en las páginas propias de Brújula (nueva pestaña, preferencias, bienvenida, novedades).

## Arreglos

- **Descargas de programas y scripts** (de la 0.3.1): ya se pueden bajar `.AppImage`, `.sh`, `.deb`… con confirmación y sin permiso de ejecución.
- Los diálogos ya no se recortan ni dejan de responder (el «¿Quitar el perfil?» no respondía), y llevan los colores y el degradado de tu aspecto.
- Preferencias → Esenciales: los sitios sugeridos ya no se salen por la derecha.

## Conocido

- Probado bajo un X virtual: falta verlo en un escritorio real (diálogos, tarjeta de los esenciales, arrastrar pestañas, actualización de un AppImage de verdad).
- Los esenciales no se guardan entre sesiones como pestañas: se vuelven a abrir al pulsarlos.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.4.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
