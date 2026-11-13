# Brújula 0.7.0

Solo Linux (GTK 4.20 o más, WebKitGTK 6.0). Con **AppImage**.

## Novedades

- **Las contraseñas se desbloquean solas al iniciar sesión**: la clave de tu cámara de contraseñas se guarda en el llavero del sistema, y Brújula ya no te pide la contraseña maestra cada vez que va a rellenar una. El archivo sigue cifrado en el disco, la contraseña maestra sigue existiendo, y «Bloquear» a mano la deja bloqueada hasta que pulses «Desbloquear». Se apaga en Preferencias → Contraseñas, y entonces la clave se borra del llavero.
- **Notificaciones web**: WhatsApp Web y otros sitios pueden pedir permiso para avisarte; Brújula pregunta, recuerda la respuesta por sitio incluso tras reiniciar, y los avisos salen en el escritorio (al pulsarlos, la pestaña pasa al frente). En Preferencias → Seguridad → «Notificaciones de sitios» puedes ver lo que has respondido y olvidarlo.
- **Panel del asistente de IA rediseñado**: cabecera con el modelo, pantalla de inicio con sugerencias, respuestas limpias, lo que hizo la IA en píldoras y tarjetas de permiso más claras. Los títulos de la interfaz usan ahora Sora, la fuente de display de Nodalia Design.
- **Brújula pesa menos al navegar**: la barra lateral, los esenciales y el historial ya no se vuelven a dibujar ni a escribir cada vez que una página cambia su título o carga un icono.

## Arreglos

- **Preferencias sin franja blanca**: la página enseña una sección cada vez y cambia al instante, sin el desplazamiento animado que dejaba teselas sin pintar al pulsar la barra lateral.
- **Menos parpadeo con un vídeo de fondo**: un vídeo que suena en otra pestaña (YouTube, por ejemplo) ya no hace trabajar a Brújula en cada cambio de audio.
- **Más seguridad en las páginas propias**: la nueva pestaña, Preferencias, la bienvenida y las novedades llevan una política de contenido estricta, y una web ya no puede navegar a direcciones `brujula://`.
- **Tus datos son privados**: las carpetas de Brújula y sus archivos (sesión, historial, marcadores, notas, ajustes) quedan solo para tu usuario, sin depender de la configuración del sistema.
- **Más descargas peligrosas piden confirmación**: `jnlp`, `pyw`, `pyz`, `wsh`, `wsc`, `msc`, `inf`, `scf`, `url`, `msp`, `mst`, `ade`, `adp` y `mde`.

## Instalar

AppImage: descárgalo de la release, `chmod +x Brujula-0.7.0-x86_64.AppImage` y ábrelo. Arch / CachyOS: `cd packaging/arch && makepkg -si`. Desde el código: `cargo install --path crates/brujula --root ~/.local --locked`.
