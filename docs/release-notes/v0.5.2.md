# Brújula 0.5.2

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Arreglos

- **Las sesiones ya no se pierden al reiniciar**: Brújula guardaba las cookies solo en memoria, así que cada vez que se cerraba (al actualizar, tras un fallo o al cerrar la ventana) te desconectaba de todas las webs y perdías las cookies importadas de otro navegador. Ahora se guardan en `cookies.sqlite`, dentro de la carpeta de tu perfil, y sobreviven a reinicios y actualizaciones. También en las aplicaciones web, cada una en su carpeta. Las que se perdieron no se pueden recuperar del disco porque nunca se escribieron: vuelve a importarlas una vez en 0.5.2 desde Preferencias → Importar, y a partir de ahí se quedan.
- **El historial no se puede sobrescribir por error**: si al arrancar el archivo del historial no se puede leer, Brújula lo aparta como `history.json.unreadable-…` y empieza uno nuevo, en vez de escribir un historial vacío encima. Así lo que había se puede recuperar.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.5.2-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
