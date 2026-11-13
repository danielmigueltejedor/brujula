# Brújula 0.5.1

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Arreglos

- **La bienvenida ya no empieza de nuevo sola**: «Bienvenido a Brújula» se escribe una vez y se queda quieto hasta que pulses Siguiente. Antes, al terminar de descargarse las listas del bloqueador en el primer arranque, el navegador recargaba la página que tenías delante, y la bienvenida volvía a escribirse desde el principio (y perdía el paso en el que estabas). Ahora las páginas propias de Brújula (bienvenida, página nueva, preferencias, novedades) no se recargan solas.
- **«¿Quién eres?» ya no sale cortado**: el aro de la imagen elegida y el borde del campo con el foco se veían recortados por el borde izquierdo. Los pasos tienen ahora sitio para ellos, y siguen alineados con los botones de abajo.
- **Sin halos ni brillo de fondo en la bienvenida**: el campo con el foco y los esenciales elegidos solo cambian el color del borde, y el fondo ya no tiene el degradado de color, como el resto de Nodalia Design.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.5.1-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
