# Brújula en Rust (reescritura)

Brújula pasa de ser un fork de GNOME Web (Epiphany, C + libadwaita) a una app propia en Rust con
[NDL](https://github.com/danielmigueltejedor/nodalia-design) (`nodalia-gtk`, GTK 4 sin libadwaita)
y WebKitGTK 6.0. El código de Epiphany sigue en el repositorio, sin tocar, hasta que se decida
retirarlo.

## Estructura

| Crate | Qué es |
| --- | --- |
| `crates/brujula-core` | La lógica sin interfaz: modelo de pestañas, barra de direcciones, listas del bloqueador, ajustes y sesión. Se prueba sin GTK ni WebKit. |
| `crates/brujula` | La app: ventana NDL, barra lateral de pestañas, vistas web y el bloqueador conectado a WebKit. |

## Decisiones

- **Todo en vertical, como Zen o Arc.** La barra lateral (isla NDL) lleva atrás/adelante/
  recargar, el escudo del bloqueador, el menú, la "píldora" de dirección y la lista de pestañas
  (ancladas arriba, cierre con clic central, menú contextual). `Ctrl+B` la ancla o la suelta:
  suelta, la web ocupa toda la ventana y la barra lateral aparece flotando al acercar el ratón
  al borde izquierdo. Los controles de ventana van arriba a la derecha, en una barra que baja al
  acercar el ratón al borde superior.
- **Spotlight.** Pulsar la píldora (o `Ctrl+L`) abre un buscador centrado sobre la página: una
  búsqueda con el buscador elegido, "ir a" si es una dirección, y las pestañas abiertas que
  coinciden. Flechas para moverse, Enter para ir, Esc para cerrar.
- **Nueva pestaña.** Página propia, limpia, sin scripts, con un cuadro de búsqueda conectado al
  buscador elegido. En el primer arranque pide elegir entre Google, DuckDuckGo, Bing, Brave
  Search, Ecosia, Startpage y Qwant; se puede cambiar desde la propia página o el menú.
- **Bloqueador.** Las listas en sintaxis ABP/uBlock (EasyList, EasyPrivacy, uBlock filters,
  privacidad, badware, cookies, español) son la única fuente. De ellas se derivan:
  - reglas de bloqueo de red en formato *content blocker*, que WebKit aplica dentro del proceso
    antes de enviar la petición (con el motor `adblock` de Brave como compilador);
  - reglas cosméticas (ocultar elementos), por sitio y genéricas, inyectadas como hoja de estilo
    en un mundo aislado de JavaScript. A WebKit **no** se le pasan las reglas de ocultar del
    conversor: las aplica en todas las webs e ignora las excepciones (`#@#`), y llegó a ocultar
    el logo y los iconos de YouTube;
  - *scriptlets* (`sitio##+js(nombre, args)`): scripts que se ejecutan en la página antes que los
    suyos, para quitar por ejemplo los anuncios de vídeo de YouTube. El código es el de uBlock
    Origin (GPL-3.0), empaquetado por `scripts/bundle-ubo-scriptlets.py` en
    `crates/brujula/resources/ubo-scriptlets.js.txt` (el resultado se versiona: compilar no necesita
    red). Solo se usan reglas con sitio, y el script solo se inyecta en esos sitios.
  Lo compilado se guarda en caché, así que desde la segunda vez las reglas están puestas antes de
  cargar la primera página. Las listas se refrescan cada 24 h (14 días con conexión de pago).
- **Sesión.** Se restauran las pestañas (solo `http`, `https` y `about:blank`); solo carga la
  que está delante, las demás al abrirlas.
- **Seguridad.** Las descargas con nombre limpio y sin archivos ejecutables (scripts, lanzadores,
  instaladores, paquetes); `javascript:`, `data:` y `blob:` no se cargan desde la barra, la línea
  de comandos ni la sesión; permisos denegados hasta que haya avisos; errores de certificado
  siempre fallan; una web no puede llevar el navegador a `file:`. El sandbox de WebKit sigue
  activo (no se define `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS`).
- **ID de la app.** `dev.danielmiguelt.Brujula`.

## Compilar y probar

Hace falta GTK >= 4.20 y WebKitGTK 6.0 (en Ubuntu 26.04: `libgtk-4-dev libwebkitgtk-6.0-dev`).

```sh
cargo test -p brujula-core      # no necesita GTK
cargo build -p brujula
cargo run -p brujula -- https://example.com
```

`BRUJULA_LISTS_DIR=<carpeta con listas> cargo test -p brujula-core real_lists -- --nocapture`
compila listas reales y comprueba que el JSON resultante es válido.

## Qué está verificado

- Núcleo: 56 tests unitarios.
- App: compilada con GTK 4.22, libadwaita 1.9 (solo como dependencia de WebKit) y WebKitGTK 2.52 y
  ejecutada bajo Xvfb: abre, restaura pestañas, abre y cierra, cambia de pestaña, oculta y muestra
  la barra lateral, y con 139 126 reglas reales WebKit bloquea una petición y se ocultan elementos
  (los del sitio y los genéricos), tanto en un primer arranque como con la caché.
- Descarga de listas: probada solo hasta el cliente HTTP; el entorno de pruebas no llegaba a los
  servidores de las listas, así que **la descarga real y el refresco están sin verificar**.

## Pendiente

- Arrastrar para reordenar pestañas (el modelo ya lo soporta), grupos de pestañas.
- Marcadores, historial, contraseñas, sincronización, extensiones: todo lo que daba Epiphany.
- Permisos (cámara, ubicación, notificaciones), pantalla completa, impresión, descargas con
  interfaz, modo privado, buscador configurable.
- Página de nueva pestaña, página de error, traducciones (hoy, textos en español).
- Excepciones del bloqueador por sitio y contador de bloqueados.
- Historial y marcadores en el Spotlight (hoy solo busca en las pestañas abiertas).
- Fijar `nodalia-gtk` a una versión estable cuando NDL la publique.
