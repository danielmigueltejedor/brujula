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

## Apariencia, espacios y marcadores

- **Preferencias** (`brujula://settings`, Ctrl+,): tema, color de acento, color del marco,
  transparencia, redondeo, separación, ancho de la barra lateral, disposición «islas» o «marco»
  (una sola isla, la web, rodeada por la barra lateral o la superior, como Zen), pestañas
  verticales u horizontales, buscador, bloqueador, listas y sitios exentos, espacios y marcadores.
  La página cambia ajustes navegando a `brujula://set?clave=valor`; el navegador solo lo atiende si
  viene de esa misma página y `Settings::apply` valida cada valor (rangos, colores `#rrggbb`).
  La apariencia se aplica en caliente con una segunda hoja de estilos y el `StyleManager` de NDL.
- **Borde inteligente** (`edge.rs`): con la barra lateral sin anclar, aparece con un movimiento
  rápido hacia el lateral (a menos de ~140 px) o con uno deliberado cerca (~48 px), no solo al tocar
  el borde.
- **Espacios** (`spaces.rs`, `icons.rs`): grupos de pestañas con nombre e icono de Nodalia Design; cada pestaña vive en uno, el
  selector está al pie de la barra, se guardan en la sesión y ⌥↑/⌥↓ cambian de espacio. Cerrar la
  última pestaña de un espacio deja una en blanco; el cierre y la navegación no salen del espacio.
  El icono se elige de una rejilla de 27 (persona, casa, maletín, mochila, birrete…), en el editor
  del espacio (clic derecho, o al crearlo) o en Preferencias. Los que NDL aún no tiene (mochila,
  maletín, birrete, mando, bolsa, café) están dibujados en `icons.rs` con las reglas de NDL (rejilla
  de 24, margen de 3, trazo de 1,75 redondeado) y la app los instala en la caché del tema de
  iconos como `ndl-<id>-symbolic`; hay un test que avisa cuando NDL los incorpore. Los iconos
  guardados por versiones anteriores (emoji) pasan a ser el de por defecto.
- **Nueva pestaña** (`newtab.rs`): saludo según la hora, buscador con el icono del motor elegido,
  marcadores como mosaicos y pestañas recientes (abiertas en el espacio y cerradas hace poco; las
  abiertas se activan en vez de duplicarse). Sin scripts; solo enlaces `https:` y `brujula://tab/<id>`.
- **Barra lateral**: 232 px por defecto, de 180 a 420; los botones de navegación son compactos para
  que el control deslizante pueda estrecharla de verdad.
- **Marcadores** (`bookmarks.rs`): estrella de la barra o Ctrl+D; solo páginas web; se buscan en el
  Spotlight (que sin texto los lista) y se gestionan en Preferencias.
- **Spotlight**: se abre en el centro de la ventana y se queda ahí mientras salen los resultados; muestra el icono de cada sitio (caché en disco por dominio) y encuentra ajustes por
  nombre o palabra clave («transparencia», «pestañas arriba»…).

## Qué está verificado

- Núcleo: 152 tests unitarios.
- Apariencia, marco, pestañas horizontales, espacios, Spotlight con iconos y ajustes, y la página de
  preferencias, comprobados bajo Xvfb con capturas (sin pantalla real ni compositor: la
  transparencia no se ha visto).
- App: compilada con GTK 4.22, libadwaita 1.9 (solo como dependencia de WebKit) y WebKitGTK 2.52 y
  ejecutada bajo Xvfb: abre, restaura pestañas, abre y cierra, cambia de pestaña, oculta y muestra
  la barra lateral, y con 139 126 reglas reales WebKit bloquea una petición y se ocultan elementos
  (los del sitio y los genéricos), tanto en un primer arranque como con la caché.
- Descarga de listas: probada solo hasta el cliente HTTP; el entorno de pruebas no llegaba a los
  servidores de las listas, así que **la descarga real y el refresco están sin verificar**.

## Pendiente

- Arrastrar para reordenar pestañas (el modelo ya lo soporta), grupos de pestañas.
- Historial, contraseñas, sincronización, extensiones: todo lo que daba Epiphany. Carpetas de
  marcadores e importación.
- Permisos (cámara, ubicación, notificaciones), pantalla completa, impresión, descargas con
  interfaz, modo privado, buscador configurable.
- Página de nueva pestaña, página de error, traducciones (hoy, textos en español).
- Excepciones del bloqueador por sitio y contador de bloqueados.
- Historial en el Spotlight (hoy: pestañas abiertas, marcadores y ajustes).
- Fijar `nodalia-gtk` a una versión estable cuando NDL la publique.
