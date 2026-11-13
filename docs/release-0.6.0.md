# Brújula 0.6.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Novedades

- **Llaves de acceso con el teléfono**: cuando una web pide una llave de acceso (passkey), Brújula muestra un código QR; lo escaneas con el teléfono, que se encuentra con el equipo por Bluetooth, y la llave se usa desde el teléfono sin que su clave salga de él. Hace falta Bluetooth en el equipo. Se puede apagar en Preferencias → Contraseñas.
- **Nueva pestaña con el Spotlight**: la página de nueva pestaña con su cuadro de búsqueda ya no existe. Ctrl+T (y el botón de nueva pestaña) abre el Spotlight sobre una pestaña en blanco, con tus marcadores como iconos, igual que los esenciales. Escribe para buscar o ir a una página; Escape cancela y la pestaña en blanco desaparece.
- **Códigos 2FA en las contraseñas**: cada login puede guardar el secreto de sus códigos de verificación en dos pasos (la clave que enseña la web junto al QR, o la dirección `otpauth://`). Se guarda cifrado con el resto, Preferencias → Contraseñas muestra el código con su cuenta atrás, y Brújula lo rellena cuando tocas el campo que lo pide, solo en el sitio exacto donde lo guardaste.
- **Esenciales reordenables**: arrástralos para cambiarlos de sitio; una línea marca dónde caerán.
- **Botón de nueva pestaña justo encima de la lista de pestañas** en vertical, y la barra lateral abre por defecto a 200 px.
- **Pantalla completa de verdad**: al poner un vídeo (YouTube, por ejemplo) a pantalla completa, desaparecen la barra lateral, las barras y el marco, y al salir todo vuelve a su sitio.

## Arreglos

- **Las webs ya no creen que usas un Mac**: Brújula se presenta siempre como Linux, en vez de la identidad de Macintosh que WebKitGTK usa con algunos sitios.
- **«Seguir al sistema» acierta con el tema**: se lee también el ajuste del escritorio y se vuelve a preguntar al portal si tardó en contestar, para que Brújula no se quede en claro con el escritorio en oscuro.
- **Sin aro de color en los campos con foco**: un campo de texto con foco solo cambia el color de su borde.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.6.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
