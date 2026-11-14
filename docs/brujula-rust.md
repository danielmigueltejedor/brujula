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
- **Seguridad.** Las descargas con nombre limpio; los archivos ejecutables (scripts, lanzadores,
  instaladores, paquetes, AppImage) piden confirmación y se guardan sin permiso de ejecución; `javascript:`, `data:` y `blob:` no se cargan desde la barra, la línea
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
- **Barra lateral**: 200 px por defecto, de 160 a 420; los botones de navegación son compactos para
  que el control deslizante pueda estrecharla de verdad.
- **Carpetas** (`folders.rs`, `browser/folders.rs`): filas plegables entre las pestañas de la barra, por espacio. Manuales (clic derecho en una pestaña → «Mover a la carpeta…» / «Nueva carpeta con esta pestaña») o **inteligentes**: una regla por sitio (`github.com`) o por texto (título o dirección) recoge solas las pestañas que coinciden; sacar una pestaña a mano la deja fuera. Se guardan en la sesión.
- **Restauración de pestañas**: la sesión se guarda al cerrar la ventana y también 1,5 s después de cada cambio (pestañas, espacios, carpetas), así que sobrevive a un cierre brusco o a un `kill`; al abrir vuelven todas, con su espacio, carpeta y ancladas, y solo carga la que está delante.
- **Sincronización** (`sync.rs`, `cloud.rs`, `browser/cloudsync.rs`): marcadores, espacios, carpetas y pestañas abiertas entre dispositivos por medio de las **cuentas en la nube de Nodalix** (Ajustes las da de alta; `~/.config/nodalix/cloud.json`, montadas con rclone en `~/Nube/<nombre>`: Google Drive, OneDrive, Dropbox, iCloud Drive). Brújula no inicia sesión en ningún servicio ni guarda tokens: solo lee y escribe una carpeta ya montada. Cada dispositivo escribe **solo su archivo** (`<nube>/Brujula/<id del dispositivo>.json`) y lee los de los demás, así que nunca hay dos escritores sobre el mismo archivo. Mezcla: marcadores por hora de guardado y de borrado (con lápidas de 90 días); espacios y carpetas por nombre, solo se añaden; las pestañas de otros dispositivos solo se listan en la página de nueva pestaña, nunca se abren solas. Una ronda al arrancar, cada 45 s y con «Sincronizar ahora»; no sube nada si no cambió. Seguridad: los archivos de la nube no son de fiar (solo `<id>.json` regulares, ≤ 2 MiB, ≤ 20 dispositivos, solo direcciones `http(s)`, texto limpio, el id del archivo debe coincidir con el de dentro) y no se escribe si la carpeta no es un punto de montaje real (si no, se llenaría el disco local). Los datos van **sin cifrar** a la nube. `BRUJULA_SYNC_HOME` y `BRUJULA_SYNC_SKIP_MOUNT_CHECK` solo son para pruebas.
- **Sincronizar con una carpeta**: si no hay cuentas de Nodalix (o no se quieren usar), Preferencias → Sincronización ofrece las carpetas de clientes de sincronización que existan en la carpeta personal (Dropbox, Nextcloud, ownCloud, OneDrive, Google Drive, iCloud Drive, MEGA, pCloud, Sync, Syncthing, Insync) y «Elegir carpeta…» (diálogo del sistema). Misma mezcla y mismo reparto que con una cuenta; elegir carpeta quita la cuenta y al revés. La página nunca dice una ruta: pide el diálogo o un número de la lista que ofrece el navegador, y la ruta guardada se valida (absoluta, sin `..` ni control). Sin comprobación de punto de montaje: la carpeta es de un cliente ajeno y local; si desaparece, la sincronización avisa y no escribe. **Las contraseñas no se sincronizan** (ni salen de este equipo).
- **Pestañas dormidas** (`sleep.rs`, `browser/sleep.rs`): una pestaña sin usar 10 min / **30 min** (por defecto) / 1 h / 3 h pierde su página (se sustituye la vista por una vacía) y se vuelve a cargar al abrirla, igual que las restauradas de la sesión. Sigue en la barra con su título. No duermen la de delante, las que suenan o cargan, las que no son web, las de vista dividida ni las flotantes. Se pierde lo no guardado de la página (formularios, posición).
- **HTTPS primero** (`https.rs`): una dirección `http://` de un sitio público (con punto, sin puerto, sin usuario, no `.local`/`.lan`/`.internal`…, no IP) se prueba como `https://`; si el sitio no responde por HTTPS se abre la original y se recuerda hasta cerrar. Un error de certificado **no** hace volver a HTTP. Solo GET. Se apaga en Preferencias → Seguridad.
- **Tema Sistema** (`browser/system_theme.rs`): NDL lee `gtk-interface-color-scheme`, pero forzar Claro/Oscuro escribe ese mismo ajuste y «Sistema» se quedaba en el último forzado. Ahora la preferencia real se lee del portal (`org.freedesktop.appearance color-scheme`, y se sigue su señal) y «Sistema» se aplica como claro u oscuro forzado según ella. **Sin probar contra un portal real** (en la VM de pruebas no hay).
- **Degradado del marco** (`frame_gradient`, `frame_angle`): 2–4 colores y ángulo múltiplo de 45; diez degradados hechos y los colores uno a uno en Preferencias → Apariencia. Sustituye al color liso; la tinta del texto sobre el marco sale de la media de los colores. Se ve en el marco y en los huecos entre islas.
- **Buscadores**: el icono de cada uno es su glifo (Simple Icons, CC0; el de Bing, del logo oficial) en blanco sobre su color.
- **Barra horizontal**: con pestañas arriba, la fila de dirección va arriba (con los controles de ventana) y las pestañas debajo; las pestañas son cajas con su propio botón de cerrar (un botón dentro de un botón no recibe sus clics) y no se reconstruyen si lo que muestran no cambió. En *Islas* la barra es su propia isla y la página otra debajo; en *Marco* la barra va sobre el color del marco que rodea la página (clase `brujula-topbar`/`brujula-page`).
- **Archivo automático** (`archive.rs`, `browser/archive.rs`): las pestañas que llevan sin usarse más de 12 h / 1 día (por defecto) / 1 semana / 30 días se cierran solas y quedan en «Archivo» (Preferencias), de donde se abren o se quitan; salen también en la nueva pestaña como «Archivada». No se archiva lo anclado, lo que está en una carpeta (manual o inteligente), la pestaña de delante, lo que suena o carga, ni lo que no es web. Solo cuenta el tiempo con el navegador abierto (la sesión guarda cuándo se escribió).
- **Vista dividida** (`browser/split.rs`): clic derecho en una pestaña → «Ver junto a la pestaña actual». Dos páginas en un `Paned` que es una página de la pila; la pestaña con el foco es la seleccionada (barra de direcciones y botones la siguen; hacer clic en la otra la selecciona). Se deshace con «Salir de la vista dividida» o cerrando una de las dos. No se guarda en la sesión.
- **Peek** (`browser/peek.rs`): Shift+clic en un enlace lo abre en una tarjeta sobre la página («Abrir en pestaña», Esc o clic fuera para cerrar). Misma sesión de red, mismo bloqueador y mismos ajustes de seguridad que una pestaña (`harden`); sin ventanas nuevas y solo direcciones web.
- **Notas** (`notes.rs`, `browser/notes.rs`): un bloc por espacio (botón arriba a la izquierda de la barra lateral), guardado solo en local en `notes.json`.
- **Boosts** (`boosts.rs`, `browser/boosts.rs`): por sitio (y sus subdominios), modo oscuro y CSS propio, en Preferencias → Boosts o desde el menú de la pestaña. Solo estilo, nunca scripts: se inyecta como hoja de estilo de usuario de WebKit con lista de sitios permitidos. Va a nivel de usuario: para ganar al CSS de la web hay que poner `!important`.
- **Ventana flotante** (`browser/float.rs`): «Sacar a una ventana flotante» pasa la página de una pestaña a una ventana pequeña propia (seguirá sonando/reproduciendo) y deja un aviso con un botón para devolverla. Es lo más cercano a imagen en imagen que permite WebKitGTK, que no tiene la API; «siempre encima» depende del gestor de ventanas.
- **Marcadores** (`bookmarks.rs`): estrella de la barra o Ctrl+D; solo páginas web; se buscan en el
  Spotlight (que sin texto los lista) y se gestionan en Preferencias.
- **Spotlight**: se abre en el centro de la ventana y se queda ahí mientras salen los resultados; muestra el icono de cada sitio (caché en disco por dominio) y encuentra ajustes por
  nombre o palabra clave («transparencia», «pestañas arriba»…).

## Herramientas de página y movimiento

- **Acciones de la ventana** (`browser/pagetools.rs`, `browser/find.rs`): `win.find`, `win.zoom-in|out|reset`, `win.copy-link`, `win.screenshot`, `win.print`, `win.toggle-mute`, `win.inspector`, `win.fullscreen`, `win.goto-tab(n)`. Los atajos están en `install_app_actions`; el menú, el Spotlight y los atajos ejecutan la misma acción.
- **Zoom por sitio** (`zoom.rs`): `zoom.json` en los datos del perfil (host → %, máx. 500 sitios); se aplica al empezar a cargar una página; no se guarda en ventanas privadas ni para páginas propias (`brujula://`).
- **Comandos del Spotlight** (`commands.rs`): con al menos dos letras que empiecen una palabra del título o de las palabras clave (sin acentos), salen hasta 3 comandos tras la primera fila. Cada uno lleva el nombre de la acción (`Action::Command`).
- **Avisos** (`browser/toast.rs`): una nota que sube sobre el pie de la página, atada a una pestaña (se va al cambiar) o efímera (`flash`). La usan el DRM, el zoom, copiar el enlace y las capturas.
- **Colores de espacio** (`spaces.rs`, `browser/extras.rs`): cada espacio puede tener un color de una paleta de 8 (`#rrggbb`, validado al leer y al guardar); la barra lateral se tiñe con un degradado suave (`tint_css`, una regla CSS generada solo con colores válidos) y al cambiar de espacio el degradado sube por la barra (`@keyframes brujula-sweep`, 650 ms, una sola vez: nada anima mientras no cambias de espacio). Se elige en el editor del espacio (clic derecho) o en Preferencias → Espacios. No se aplica en ventanas privadas.
- **Cambio de espacio y de pestaña**: las filas del espacio que entra se deslizan (como las pestañas nuevas) y la página que pasa al frente se aclara en 150 ms (`brujula-enter`, desde el 55 % de opacidad para no dejar un destello en blanco).
- **Enlaces de otras aplicaciones** (`quick_links`, activado por defecto): con Brújula abierta, un enlace que llega de fuera (una sola dirección web) se ve en la tarjeta de Peek con «Abrir en pestaña»; si no, se abre una pestaña como antes. Se apaga en Preferencias → Archivo.
- **Fluidez**: desplazamiento suave y caché de páginas (Atrás/Adelante) de WebKit activados en todas las vistas.
- **Movimiento**: solo CSS (`@keyframes` y `transition` al final de `style.css`): tarjetas que suben y se aclaran al abrirse (Spotlight, Peek, avisos, buscar), pestañas nuevas que entran desde el lado, fundidos de color y levitación suave en esenciales y espacios. GTK lo apaga solo si el escritorio pide sin animaciones. **Sin verlo en pantalla**: se comprobó que el CSS nuevo se lee sin errores en GTK 4.14, no cómo se ve en 4.20.

## Extensiones de Chromium

Código: núcleo sin interfaz en `crates/brujula-core/src/ext/` (manifiesto, paquetes `.crx`/zip, patrones de coincidencia, textos `_locales`, reglas `declarativeNetRequest`, registro, `shim.js` con la API `chrome.*`, `css.rs`) y motor en `crates/brujula/src/browser/ext*.rs`.

- **Instalar** (Preferencias → Extensiones): desde una carpeta con `manifest.json`, un `.crx`/`.zip` o la dirección (o id) de una página de la Chrome Web Store (se descarga el `.crx` de `clients2.google.com`). Se desempaqueta en un directorio temporal, se lee, y **se pide confirmación con lo que podrá hacer** antes de copiarla a `~/.local/share/brujula/extensions/<id>/pkg`. Las firmas del `.crx` no se comprueban: instala el usuario.
- **Qué funciona**: scripts y CSS de contenido (`matches`, `exclude_matches`, `run_at`, `all_frames`, `world: MAIN`), fondo (service worker o scripts MV2 en una página oculta; `importScripts` por XHR síncrono), ventana emergente en un popover del botón de la barra, página de opciones (ventana propia), `chrome.runtime` (mensajes, puertos, `getURL`, `getManifest`, `onInstalled`), `chrome.storage` (local/sync/session, en disco, con `onChanged`), `chrome.tabs` (query, get, create, update, remove, reload, sendMessage, connect, eventos), `chrome.scripting.executeScript/insertCSS` y sus equivalentes MV2, `chrome.action`/`browserAction` (título, popup, badge), `chrome.i18n`, `chrome.alarms`, `fetch` entre orígenes con los permisos de host del manifiesto, y reglas estáticas `declarativeNetRequest` (bloqueo, permitir y subir a HTTPS) convertidas a bloqueador de contenido de WebKit.
- **Qué no**: `webRequest` (ni bloqueante), redirecciones y cambios de cabeceras de DNR, reglas dinámicas, scripts de contenido registrados en vuelo, menús contextuales, cookies, historial, marcadores, notificaciones, mensajería nativa, gestores de contraseñas. Esas API aceptan la llamada y no hacen nada, para que una extensión que las usa de refilón siga funcionando. **No todas las extensiones funcionarán** (ni siquiera todas las MV3).
- **Aislamiento**: los scripts de contenido corren en un mundo propio (`brujula-ext-<id>`) del gestor de contenido de las pestañas y solo ese mundo ve el manejador `brujulaExt_<id>`; las páginas de la extensión (fondo, ventana emergente, opciones) usan un gestor propio por extensión, nunca muestran web (cualquier navegación fuera de `brujula-ext://<id>/` se bloquea; un enlace abre una pestaña) y solo ellas pueden leer todos los archivos; una web solo lee lo que el manifiesto marca como `web_accessible_resources`. Comprobado: una web no ve el manejador ni `chrome`, y no puede cargar archivos de la extensión.
- WebKit solo admite hojas de usuario a nivel de usuario (las de autor no se aplican), así que el CSS de las extensiones se reescribe con `!important` (`css.rs`) para que gane a la web como en Chrome.
- Los ids de pestaña empiezan en 1 (en el modelo, en 0).

## Qué está verificado

- Núcleo: 213 tests unitarios.
- Apariencia, marco, pestañas horizontales, espacios, Spotlight con iconos y ajustes, y la página de
  preferencias, comprobados bajo Xvfb con capturas (sin pantalla real ni compositor: la
  transparencia no se ha visto).
- App: compilada con GTK 4.22, libadwaita 1.9 (solo como dependencia de WebKit) y WebKitGTK 2.52 y
  ejecutada bajo Xvfb: abre, restaura pestañas, abre y cierra, cambia de pestaña, oculta y muestra
  la barra lateral, y con 139 126 reglas reales WebKit bloquea una petición y se ocultan elementos
  (los del sitio y los genéricos), tanto en un primer arranque como con la caché.
- Sincronización: probada bajo Xvfb con una carpeta de nube simulada y un segundo dispositivo falso (llegan marcador, espacio con icono, carpeta y pestañas; su archivo se escribe; sin montaje no se escribe nada). **Sin probar con rclone ni una nube real.**
- Archivo, vista dividida, Peek, notas, Boosts (hoja de usuario y modo oscuro) y ventana flotante: probados bajo Xvfb con un servidor local; sin compositor ni gestor de ventanas, así que el aspecto de la ventana flotante y de las transparencias no se ha visto en un escritorio real.
- Sincronizar con carpeta (se escribe el archivo del dispositivo en la carpeta elegida; la página se vuelve a pintar en la sección), pestañas dormidas (duerme y recarga con servidor local, con tiempos acortados temporalmente), HTTPS primero (se ve el `ClientHello` en el puerto 443 y, al no haber TLS, se abre la original) y la barra horizontal en Islas y Marco: probados bajo Xvfb. **El diálogo «Elegir carpeta…» y una nube real, sin probar.**
- Extensiones: probadas bajo Xvfb con una extensión MV3 propia (`tests/fixtures/test-extension`): script y CSS de contenido, mensajes en ambos sentidos, puertos, storage con `onChanged`, `tabs.query`, `scripting.executeScript`, `fetch` con permisos de host, bloqueo de un script por regla DNR, ventana emergente, página de opciones, e instalación desde carpeta con el diálogo de permisos y la sección de Preferencias. **No se ha probado ninguna extensión real de la Chrome Web Store ni la descarga desde la tienda** (el entorno de pruebas no llegaba), ni `.crx` firmados de verdad más allá de los tests unitarios del formato.
- Descarga de listas: probada solo hasta el cliente HTTP; el entorno de pruebas no llegaba a los
  servidores de las listas, así que **la descarga real y el refresco están sin verificar**.

## Historial y ventana privada

- **Historial** (`brujula-core/src/history.rs`, `browser/history.rs`): las páginas web visitadas, de la más reciente a la más antigua, hasta 5000, en `<datos>/history.json`. Solo guarda direcciones `http(s)`, así que un archivo manipulado no puede hacer abrir otra cosa. Se escribe unos segundos después de la última página. Se apaga y se borra (última hora, 24 horas, todo, o una a una) desde Preferencias → Historial. El Spotlight lo ofrece como una fuente más, detrás de pestañas y marcadores, sin repetir lo que ya está en esas listas. El historial no se sincroniza.
- **Ventana privada** (Ctrl+Mayús+N, el menú, o `brujula --private`): un `Browser` con `private = true`. Usa una `NetworkSession` efímera (cookies, caché y almacenamiento en memoria), parte de una sesión vacía y no escribe nada: ni la sesión, ni los ajustes, el historial, los marcadores, las notas, los Boosts, el archivo, los iconos de sitios ni las extensiones (todas las funciones `save_*` salen al principio). No arranca la sincronización, el archivo automático ni las extensiones, y las preferencias solo se abren en la ventana normal. Lo que sí hace: leer los marcadores y las reglas del bloqueador ya compiladas. Verificado a mano: abrir una página en una ventana privada no crea `history.json` ni toca `session.json`. **Sin verificar**: que WebKit no deje nada en disco fuera de lo que gestiona `NetworkSession` (el entorno de pruebas no tiene un perfil real para comparar) y que `brujula --private` con una instancia ya abierta reciba la dirección (hoy, ahí, solo abre la ventana).

## Contraseñas

- **Bóveda** (`brujula-core/src/vault.rs`): un archivo, `<datos>/passwords.vault` (permisos 0600). La contraseña maestra pasa por Argon2id (64 MiB, 3 pasadas, 1 hilo; al abrir un archivo se comprueba que el coste leído no pase de 1 GiB y 10 pasadas, para que un archivo manipulado no cuelgue el navegador) y da una clave de 32 bytes; el contenido se cifra entero con XChaCha20-Poly1305 con un nonce aleatorio nuevo en cada escritura, así que ni los sitios ni los usuarios se ven en el archivo. Mínimo 8 caracteres. Las claves y las contraseñas se borran de memoria al soltarlas (`zeroize`). Un inicio de sesión pertenece a UN origen: `https://` (o `http://` solo en `localhost`, `127.0.0.1` y `[::1]`), con el puerto, y no se comparte con subdominios.
- **Página** (`browser/passwords.rs` y `passwords.js`): un script en un mundo aislado propio, solo en el marco principal, que la página no puede ver ni llamar. Detecta el envío de un formulario con contraseña y se lo cuenta al navegador, que pregunta con un diálogo nativo (Guardar / Ahora no / Nunca para este sitio). Para rellenar, el script pide la lista de usuarios del sitio y, **solo ante un clic o una tecla reales** (`event.isTrusted`), pide la contraseña del elegido; el navegador la entrega únicamente si la bóveda está abierta y el origen del script coincide con el guardado y con alguna pestaña abierta. Con la bóveda cerrada pide desbloquear una vez por sitio. La bóveda se cierra sola a los 10 minutos sin usarla y al cerrar la ventana.
- **Desbloqueo con el inicio de sesión** (`keyring.rs`, ajuste `passwords_keyring`, activado por defecto): la clave de 32 bytes que cifra el archivo (`Vault::export_key`, en base64) se guarda en el llavero del sistema (Secret Service: GNOME Keyring, KWallet…) con `secret-tool`, atributos `service=brujula kind=vault-key profile=<perfil>`, por la entrada estándar (no sale en la lista de procesos). Al arrancar (y cuando una página toca un campo de acceso con la bóveda cerrada) Brújula lee la clave y abre el archivo con `Vault::open_with_key`, sin Argon2 y sin pedir la maestra; el llavero se abre al iniciar sesión, así que la bóveda queda lista nada más entrar al equipo. El archivo sigue cifrado en el disco: quien copie solo el archivo, o un respaldo, no lo lee. La contraseña maestra se sigue pidiendo si el llavero no tiene la clave (primer arranque tras activar esto, otro equipo, llavero reiniciado, clave que ya no corresponde porque la maestra cambió en otro sitio: esa entrada se borra y se guarda la nueva), y se guarda la clave cada vez que se crea la bóveda, se desbloquea con la maestra o se cambia la maestra. Ventanas privadas: nada de esto. «Bloquear» a mano la deja bloqueada hasta que se pulse «Desbloquear»; el bloqueo por inactividad (10 min) y el de cerrar la ventana la vuelven a abrir solos en el siguiente uso. Desactivar el interruptor borra la clave del llavero; «Borrar…» también. **Riesgo asumido**: cualquier programa que corra con tu sesión y pueda leer el llavero (sin sandbox) puede pedir la clave, como pasa con los navegadores que usan el llavero del sistema. **Sin verificar en una sesión real**: se probó la lógica (`cargo test`) y que compila, no el llavero de un escritorio real.
- **Preferencias → Contraseñas**: interruptor, crear/desbloquear/bloquear/cambiar la maestra, lista de sitios y usuarios (nunca las contraseñas), Copiar (se quita del portapapeles a los 30 s), Quitar, sitios con «nunca», y borrar todo (lo único que queda si se olvida la maestra).
- Una ventana privada no tiene nada de esto. **Sin verificar**: formularios que no se envían de forma normal (algunas aplicaciones de una sola página), el comportamiento con varios usuarios para un sitio fuera de la prueba de «uno solo», y contraseñas de páginas dentro de iframes (el script solo mira el marco principal, a propósito). Importar desde otros navegadores (CSV) aún no existe.

## Llaves de acceso (passkeys)

WebKitGTK no trae WebAuthn: `navigator.credentials` y `PublicKeyCredential` no existen en una página de Brújula hasta que Brújula los pone. Se comprobó con WebKitGTK 2.52: sin ellos, `typeof navigator.credentials` es `undefined` en un contexto seguro.

- **Qué hace hoy**: usar la llave de acceso de un **teléfono** (Android o iPhone) para crear una llave o entrar en un sitio, por el flujo «híbrido» de FIDO (CTAP 2.2, también llamado caBLE v2): Brújula muestra un código QR, el teléfono lo escanea, anuncia su presencia por Bluetooth (BLE) y habla con el equipo por el servidor túnel de su fabricante, con un canal cifrado de extremo a extremo (Noise). La clave privada no sale del teléfono y es el teléfono quien comprueba a la persona (huella, cara, PIN). **Qué no**: guardar llaves de acceso en Brújula, llaves USB/NFC y el relleno automático de llaves en los campos de usuario (`mediation: "conditional"`); `isUserVerifyingPlatformAuthenticatorAvailable()` y `isConditionalMediationAvailable()` responden `false` para que los sitios ofrezcan «usar el teléfono» en vez de esperar una llave local.
- **El protocolo** (`crates/brujula-passkeys`, sin interfaz): lo hace `libwebauthn` (LGPL-2.1-o-posterior), de la iniciativa Credentials for Linux. La caja añade lo que un navegador tiene que decidir por su cuenta: el origen tiene que ser seguro (`https://`, o `http://` en `localhost`); `rpId` se valida contra ese origen con la lista de sufijos públicos (incluida en el binario con la caja `psl`, sin paquete del sistema), así que `evil.example.org` no puede pedir `example.com`, ni nadie `com` o `github.io`; el tiempo de espera se acota entre 1 y 10 minutos; y cada final posible es una `Reason` con su excepción del DOM (`SecurityError`, `InvalidStateError`, `NotSupportedError`, y `NotAllowedError` para todo lo que la especificación manda confundir, como «no hay llave» y «dijo que no»). La ceremonia corre en un hilo propio con un runtime de tokio que vive lo que ella dura. El QR solo anuncia el túnel (no el canal Bluetooth directo de CTAP 2.3), porque algunos teléfonos rechazan un QR con esa clave.
- **Página** (`browser/passkeys_main.js`, `passkeys.js` y `passkeys.rs`): el primer script va en el mundo de la página, solo en el marco principal, y es lo único que ve la página: clases `PublicKeyCredential`, `AuthenticatorAttestationResponse`…, y un `navigator.credentials` con `create` y `get` (el resto de pedidos, como contraseñas o `federated`, responde como un navegador que no los admite). No decide nada: serializa las opciones al JSON de WebAuthn y anuncia la petición. El segundo corre en un mundo aislado y es el único que habla con el navegador. **La página no puede decir quién es**: el navegador va a buscar las opciones a la pestaña que tiene delante (`window.__brujulaWa.take(id)`, una sola vez) y toma el origen de la dirección de esa pestaña; una pestaña en segundo plano, o una petición que no está esperando ahí, se rechaza con `NotAllowedError`. La página tiene que haberse usado alguna vez (`userActivation.hasBeenActive`), solo hay una ceremonia a la vez, y siempre se ve el diálogo, con el nombre del sitio, y hay que escanear con un teléfono que tengas en la mano.
- **Diálogo** (`ndl::AlertDialog`): el QR en una tarjeta blanca de cuadros de píxel entero (nítido en cualquier tema), el estado debajo («Esperando al teléfono…», «Conectando…», «Conectado. Confirma en el teléfono.») y «Cancelar». Si no hay Bluetooth, o falla la conexión, se explica con otro diálogo y la página recibe `NotAllowedError`, que es lo que espera.
- **Requisitos**: un adaptador Bluetooth con BlueZ (`bluetoothd`) y acceso a internet para el servidor túnel; el teléfono, cerca. Cambiar el interruptor de Preferencias → Contraseñas → «Llaves de acceso con el teléfono» vale desde la próxima vez que se abra Brújula.
- **Qué falta**: guardar llaves de acceso en la bóveda de Brújula (llaves propias, sin teléfono), llaves de seguridad USB (CTAP-HID, `libwebauthn` ya las trae) y las llaves en el desplegable de usuarios. **Sin verificar con un teléfono de verdad**: los tests automáticos cubren la validación del origen y de `rpId`, el QR, el tiempo de espera y la traducción de errores; la tubería página → navegador → página se probó a mano en una sesión X virtual (diálogo con QR, cancelar y volver a pedir, `rpId` ajeno rechazado con `SecurityError`, y el aviso de que no hay Bluetooth). El Bluetooth y el túnel hay que probarlos con un teléfono y un adaptador.

## Notificaciones web

- **Qué pasaba**: todas las pestañas rechazaban cualquier permiso, así que una web que pedía mostrar notificaciones (WhatsApp Web, un correo) recibía «bloqueado» sin que nadie viera una pregunta, y los avisos que WebKit entrega (`show-notification`) no se mostraban nunca.
- **Cómo funciona ahora** (`crates/brujula/src/notify.rs`, `brujula-core/src/sitepermissions.rs`): cuando una página pide notificaciones, Brújula pregunta («Ahora no» / «Bloquear» / «Permitir»). «Permitir» y «Bloquear» se guardan por origen (`https://sitio.com`, sin ruta) en `site-permissions.json` del perfil; «Ahora no» no guarda nada. Al arrancar WebKit se le dice qué sitios estaban permitidos (`initialize-notification-permissions`) para que `Notification.permission` sea `granted` desde la primera página y nadie tenga que volver a aceptar. Los avisos de los sitios permitidos salen como notificaciones del escritorio (`GNotification`); al hacer clic, la pestaña pasa al frente y se avisa a la página (`notification.clicked()`, así WhatsApp abre el chat).
- **Dónde se cambia**: Preferencias → Seguridad → «Notificaciones de sitios» lista lo respondido, con «Olvidar» para que el sitio vuelva a preguntar. Una respuesta olvidada deja de mostrar avisos al instante, aunque WebKit conserve el permiso hasta reiniciar.
- **Ventanas privadas**: sin notificaciones. WebKit las rechaza solo en las sesiones efímeras y Brújula tampoco guarda nada ahí.
- **Aplicaciones web**: igual, con las respuestas guardadas en el perfil de cada aplicación (`webapps/<id>/…/site-permissions.json`).
- **Solo `https://`** (y `http://localhost`): una página sin conexión segura no puede notificar.
- **Sin verificar**: el aviso en un GNOME real (aquí no hay servidor de notificaciones: se comprobó que se envía, no que se ve) ni WhatsApp Web con una cuenta; sí se probó contra WebKitGTK 2.52 real el ciclo completo (pregunta, permiso guardado y vuelto a leer tras reiniciar, bloqueo, «Ahora no», aviso, clic).

## Aplicaciones web

- **Crear una** (`brujula-core/src/webapps.rs`, `browser/webapps.rs`, `webapp.rs`): menú ⋮ → «Instalar como aplicación web…» en la página abierta (pide el nombre y usa el favicon como icono), o Preferencias → Aplicaciones web (dirección y nombre opcional). Se apunta en `<datos>/webapps.json` y se escribe un lanzador `~/.local/share/applications/brujula-webapp-<id>.desktop`. El lanzador lleva solo el **id** (letras, minúsculas, cifras y guiones, validado también al leer el archivo): `Exec="brujula" --webapp <id>`; la dirección sale del registro, nunca de la línea de órdenes. Con el AppImage, `Exec` apunta al propio AppImage.
- **La aplicación** (`brujula --webapp <id>`): un proceso aparte, con su identificador de aplicación (`dev.danielmiguelt.Brujula.WebApp.w<id>`, así el lanzador y el dock la distinguen del navegador) y **su propio perfil** (cookies, almacenamiento y caché en `<datos>/webapps/<id>`, aparte del navegador y de las demás). Ventana con atrás, adelante y recargar, sin pestañas ni barra de direcciones, y con el bloqueador (las reglas ya compiladas y las excepciones del navegador). Los enlaces del mismo sitio (mismo dominio base) se quedan en la ventana; los de otros sitios y los `mailto:`/`tel:` se abren fuera (navegador predeterminado). Las descargas siguen las mismas reglas que el navegador. Cámara y micrófono se piden con un diálogo; el resto de permisos se deniegan.
- Quitar una aplicación (Preferencias) borra su lanzador, su perfil y su entrada. El primer favicon que carga la aplicación pasa a ser el icono del lanzador si no había uno.
- **Sin verificar**: cámara y micrófono en una videollamada real, que cada lanzador agrupe bien en un escritorio real (solo se probó la ventana y la creación en un X virtual), y el inicio de sesión con ventanas emergentes (las aplicaciones no abren ventanas emergentes de la web: un «Iniciar sesión con…» que las use no funcionará). No hay contraseñas guardadas ni extensiones dentro de las aplicaciones. Las notificaciones sí (ver «Notificaciones web»).

## Esenciales, descargas y carpetas anidadas

- **Esenciales** (`essentials.rs`, `browser/essentials.rs`): hasta 12 sitios web (solo `http(s)`, sin usuario en la dirección) guardados en `essentials.json`. Con las pestañas en la barra lateral salen como una cuadrícula de iconos bajo la barra de direcciones; con las pestañas arriba, como una columna a la izquierda de la página. Un clic va a la pestaña de ese sitio en el espacio actual, o la abre. Se añaden desde el menú («Añadir a esenciales»), el menú de una pestaña o Preferencias → Esenciales (con una lista de sitios habituales); clic derecho en un icono: abrir en pestaña nueva, mover o quitar. El icono es, por orden: el icono de la aplicación del tema de iconos (Nodalix los trae para YouTube, Discord, WhatsApp, Reddit, Spotify, Netflix, Instagram, Telegram, ChatGPT y Calendar; el nombre va en la tabla de sitios sugeridos), el icono del propio sitio (se busca una vez por sitio con una página oculta, sin JavaScript ni multimedia, y se guarda) y, si no hay ninguno, una letra sobre el color de la marca. La página de bienvenida usa los mismos iconos del tema. **No** ocultan la pestaña de la lista y no son una sola pestaña compartida entre espacios (cada espacio abre la suya).
- **Descargas** (`browser/downloads_panel.rs`): botón abajo a la izquierda de la barra lateral (en la barra de arriba con las pestañas arriba) con la lista de lo descargado en la sesión: progreso, cancelar, abrir y mostrar en la carpeta. Las reglas de qué se guarda (nombre limpio, ejecutables con confirmación y sin permiso de ejecución) siguen en `downloads.rs` y `save_download.rs`. **Análisis**: cada archivo se escribe con un nombre oculto (`.nombre.brujula-part`), y al terminar la descarga (sin frenarla) `scan.rs` mira si el contenido es un programa (ELF, PE de Windows, Mach-O) aunque el nombre prometa un documento, una imagen o un vídeo, y `save_download.rs` pasa ClamAV (`clamdscan` o `clamscan`) si está instalado, con 90 s como máximo. Si algo sale, un diálogo pregunta si se borra; si no, el archivo recibe su nombre. Sin ClamAV solo hay la comprobación de contenido, que no es un antivirus. La lista no se guarda entre sesiones.
- **Carpetas anidadas** (`folders.rs`): una carpeta puede estar dentro de otra hasta tres niveles (menú de la carpeta: «Nueva subcarpeta», «Meter en la carpeta…», «Sacar de la carpeta»). Plegar una carpeta oculta lo que tiene dentro; al borrarla, las de dentro suben un nivel. Un archivo de sesión dañado (bucles, padres que no existen) se corrige al leerlo. La sincronización no lleva la jerarquía.
- **Iconos de aplicaciones web** (`tile.rs`): el icono del sitio va sobre un cuadrado redondeado (claro, u oscuro si el icono es casi blanco), como los iconos de aplicación de Colloid.
- **Diálogos** (`dialogs.rs`): avisos y preguntas con los diálogos de Nodalia Design sobre la ventana, no los de GTK.

## Bienvenida y perfiles

- **Asistente de bienvenida** (`welcome.rs`, `browser/welcome.rs`): la primera vez que se abre un perfil (no existe su `settings.json`; los ajustes anteriores a esta versión cuentan como ya vistos, `welcome_done`) se abre `brujula://welcome` en lugar de la página de nueva pestaña. Es una página con sus pasos dentro: saludo escrito como a máquina en el idioma del sistema (es, en, fr, de, pt; el resto, inglés), importar marcadores, navegador predeterminado, esenciales (se puede omitir) y crear tu usuario. Lo que necesita el navegador sale como `brujula://welcome?clave=valor` y solo lo atiende la propia página de bienvenida. Con «Terminar» se guarda el perfil, los esenciales elegidos y `welcome_done`.
- **Importar marcadores** (`bookmark_import.rs`): lee Chromium/Chrome/Brave/Edge/Vivaldi/Opera (`Default/Bookmarks`, nativos, Flatpak y Snap), Firefox, Zen, LibreWolf y Floorp (la copia más reciente en `bookmarkbackups/*.jsonlz4`, con un descompresor LZ4 propio con límites) y archivos HTML de marcadores exportados. Solo se leen los marcadores: ni historial, ni contraseñas, ni cookies. Se añaden sin duplicar (máximo 5000 por importación). Arc no tiene versión para Linux: se importa con el botón de archivo si se exporta desde el Mac.
- **Navegador predeterminado**: `AppInfo::set_as_default_for_type` para `text/html` y `x-scheme-handler/http(s)` sobre `dev.danielmiguelt.Brujula.desktop`. Si ese lanzador no está instalado, la página lo dice y señala los ajustes del sistema.
- **Perfiles** (`profiles.rs`, `avatars.rs`, `profiles.rs` de la aplicación, `browser/profiles_ui.rs`): la lista (`profiles.json`, solo lectura del propietario) está en la carpeta de configuración común. El primero se llama `default` y **conserva las carpetas de siempre**; los demás usan `<datos|caché|config>/brujula/profiles/<id>`. Cada perfil es su propia instancia (`dev.danielmiguelt.Brujula.P<id>`) y se arranca con `brujula --profile <id>`. Si hay más de un perfil, o el único tiene contraseña, al lanzar `brujula` sale «¿Quién navega?» y la aplicación se reinicia (`exec`) con el perfil elegido; `--private` lo salta. Las aplicaciones web son comunes a todos los perfiles. Preferencias → Perfiles: añadir, abrir, poner o quitar la contraseña y quitar (borra sus carpetas; el principal y el que está en uso no se pueden quitar).
- **Qué protege la contraseña de un perfil**: solo el selector (y abrir o quitar ese perfil desde Preferencias). **No cifra** los archivos del perfil ni impide `brujula --profile <id>` desde una terminal: quien tenga acceso a la cuenta del equipo puede leerlos. Se guarda con Argon2id (19 MiB, 2 pasadas, con topes al leer el archivo). Las contraseñas guardadas siguen en su bóveda cifrada, con su contraseña maestra.
- **Sin verificar**: el asistente se probó entero en un X virtual (importar de un `Bookmarks` de Chrome de prueba, esenciales, crear usuario con contraseña, selector con contraseña mala y buena, añadir y abrir un segundo perfil); no se probó importar de Firefox/Zen/archivo HTML con navegadores reales (solo con datos de prueba en los tests), el selector de archivos, poner el navegador predeterminado con el lanzador instalado, ni quitar un perfil ni cambiar su contraseña desde Preferencias en la aplicación.

## Rendimiento y endurecimiento de la interfaz (0.7.0)

- **Menos redibujados en la barra lateral**: las páginas avisan todo el rato de título, carga e icono, y cada aviso rehacía los iconos de esenciales entero (cuadrícula y columna, con sus imágenes) y volvía a ordenar y filtrar toda la lista de pestañas. Ahora los esenciales solo se rehacen cuando cambia algo que se ve (nombre, orden, icono, pestaña abierta o activa, disposición; `browser/essentials.rs`, clave `drawn`) y la lista solo se reordena cuando entra, sale, se mueve o se selecciona una pestaña, o cuando un cambio manda una pestaña a otra carpeta (`folders::layout_key`). Un título que cambia sin mover nada ya no toca la lista; y un clic a medias en un icono ya no se pierde porque la página acabe de cargar.
- **Historial**: una página que repite el mismo título (un vídeo en una pestaña de fondo, un chat) ya no pide escribir el historial entero cada vez (`History::set_title` dice si cambió algo).
- **Fuentes de las páginas propias**: `brujula-font://` lee cada tipografía del disco una vez y la guarda en memoria.
- **Política de contenido (CSP) en las páginas propias** (`csp.rs`): nueva pestaña, Preferencias, bienvenida y novedades llevan `Content-Security-Policy` (`default-src 'none'`; el único script es el que escribe el navegador, nombrado por su hash SHA-256; estilos en línea, imágenes `data:` y las fuentes de `brujula-font:`; sin red, marcos, formularios, `<base>` ni plugins). Todo el texto ya se escapa; esto es la segunda pared: un texto que se colara (el título de una página del historial, el nombre de una extensión) se vería, pero no se ejecutaría ni podría pedir cambios al navegador. Comprobado en Chromium con las cuatro páginas reales: sin infracciones, el script propio corre y un `<script>`, un `onload` o un `javascript:` metidos a mano no.
- **Una página web no puede navegar a `brujula://…`** (`is_internal_address`): así es como las páginas propias piden cambios; una web no tiene por qué hablarles.
- **Archivos y carpetas privados** (`paths.rs`): las carpetas de datos, configuración y caché se crean (o se corrigen al arrancar) con permisos 0700, y todo lo que se escribe con `write_atomic` (sesión, historial, marcadores, notas, ajustes, almacenamiento de extensiones) sale 0600 como ya hacía la bóveda. Antes dependía de la `umask` de cada equipo (normalmente 0644: legible por otros usuarios).
- **Descargas**: la lista de extensiones que piden confirmación suma `jnlp`, `pyw`, `pyz`, `wsh`, `wsc`, `msc`, `inf`, `scf`, `url`, `msp`, `mst`, `ade`, `adp` y `mde`.
- **Sin verificar**: nada de esto se ejecutó en una ventana real de Brújula (el crate de la interfaz se compiló y pasó clippy contra GTK 4.14, no se abrió). Hay que mirar a mano la barra lateral cargando páginas con esenciales, que Preferencias, la bienvenida y la nueva pestaña se ven igual que antes (tipografías incluidas), y que los interruptores de Preferencias siguen funcionando.

## Pendiente

- Arrastrar para reordenar pestañas (el modelo ya lo soporta), grupos de pestañas.
- Importar contraseñas e historial de otros navegadores (los marcadores ya se importan; la sincronización cubre marcadores, espacios, carpetas y pestañas, no historial ni contraseñas).
- Permisos de cámara y ubicación en las pestañas (las notificaciones ya se preguntan, ver abajo), pantalla completa, impresión, descargas con
  interfaz, buscador configurable.
- Página de nueva pestaña, página de error, traducciones (hoy, textos en español).
- Excepciones del bloqueador por sitio y contador de bloqueados.
- Fijar `nodalia-gtk` a una versión estable cuando NDL la publique.
