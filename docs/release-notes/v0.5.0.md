# Brújula 0.5.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Novedades

- **Importar de otro navegador, de verdad**: además de los marcadores, trae el **historial**, las **cookies** y las **contraseñas** de Chrome, Chromium, Brave, Edge, Vivaldi y Opera, y el historial, las cookies y los marcadores de Firefox, Zen, LibreWolf y Floorp. Se hace desde el asistente de bienvenida y desde la página nueva Preferencias → Importar, todo o por partes. Las contraseñas pasan a tu cámara con contraseña maestra; si el escritorio guarda la clave del otro navegador, puede pedirte permiso. Las de Firefox se importan desde el CSV que exporta (o de cualquier gestor). Lo que ya tenías no se borra ni se duplica.
- **Campos de búsqueda sin halo**: al enfocar un campo solo cambia el color del borde, sin el segundo aro azul borroso (en Nodalia Design, para todas las aplicaciones).
- **Esenciales**: como máximo tres por fila en la barra lateral.
- **Barra lateral más fina**: 180 px por defecto y hasta 160 px de mínimo (los anchos antiguos por defecto pasan al nuevo).
- **Extensiones**: los paneles que una extensión pone dentro de una página (en un iframe) ya se ven; antes salían vacíos.

## Arreglos

- Al arrastrar pestañas ya no puede quedar una pestaña flotando: el cambio de sitio se aplica cuando termina el arrastre.
- El botón de la página (por ejemplo «Añadir esta extensión» de la Chrome Web Store) también está en la barra fina que cubre la cabecera de la barra lateral, donde antes quedaba tapado.

## Conocido

- Probado bajo un X virtual: falta verlo en un escritorio real (escala fraccionaria, arrastrar pestañas, importar de tus navegadores de verdad).
- Las contraseñas de Firefox no se leen directamente: hay que exportarlas a CSV.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.5.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
