# Vídeo y música con DRM (Netflix, Disney+, Prime Video, Spotify…)

**Resumen:** el motor de Brújula, WebKitGTK, no puede reproducir contenido con DRM. Brújula
lo dice en cuanto abres un sitio así y te ofrece abrir esa página en un navegador de tu
equipo que sí lo reproduce (Chrome, Edge, Firefox, Vivaldi, Brave u Opera).

## Por qué no se reproduce dentro de Brújula

Estas plataformas cifran el vídeo y solo entregan la clave a un módulo de descifrado
(**Widevine**) que el navegador carga mediante las *Encrypted Media Extensions* (EME). Comprobado
con WebKitGTK 2.52 real, con `enable-encrypted-media` activado:

```
typeof navigator.requestMediaKeySystemAccess   →  "undefined"
typeof MediaKeys                               →  "undefined"
```

- Las compilaciones de WebKitGTK que empaquetan las distribuciones (Arch, Ubuntu, Fedora…)
  dejan EME fuera: `ENABLE_ENCRYPTED_MEDIA` solo está activo en las compilaciones
  experimentales (`OptionsGTK.cmake`).
- Aun con EME compilado, WebKitGTK no sabe cargar Widevine. La única vía es el puente
  Thunder/OpenCDM (`ENABLE_THUNDER`), que solo se activa en modo desarrollador y exige
  compilar WebKit a mano.
- Widevine es un binario cerrado de Google, con licencia. En Linux de escritorio solo está
  disponible con Chrome, Edge, Firefox y los navegadores que han firmado un acuerdo con Google.
  Redistribuirlo con Brújula necesita ese acuerdo. Además, en un navegador que no es Chrome,
  el módulo comprueba que quien lo carga está firmado por Google.
- En el mejor de los casos sería Widevine **L3** (por software): hasta 720p en la mayoría de
  plataformas.

Por eso **Brújula no puede prometer DRM dentro de la ventana** con el motor actual. Lo que sí hace:

## Lo que hace Brújula

1. **Reconoce los sitios con DRM** (`brujula-core/src/drm.rs`): Netflix, Disney+, Max, Prime
   Video, Hulu, Paramount+, Peacock, Apple TV, Crunchyroll, SkyShowtime, Movistar+, Filmin,
   Rakuten TV, DAZN, Atresplayer, Mitele, Discovery+, Starz, Spotify web, Tidal y Amazon
   Music; en Amazon solo la parte de vídeo (`/gp/video`). Puedes añadir los tuyos con el menú
   «Abrir con soporte de DRM…».
2. **Te avisa** con una nota sobre la página, una vez por sitio y sesión: «Netflix usa
   contenido con protección… Firefox sí», con el botón **Abrir en Firefox** (o el que
   tengas) y «No avisar de este sitio».
3. **Abre la página en el otro navegador**: los basados en Chromium en modo aplicación
   (`--app=<dirección>`: una ventana limpia con solo el sitio, con tu sesión de ese navegador);
   Firefox en una ventana nueva. Se elige el primero instalado que trae Widevine (Chrome,
   Edge), luego los que lo descargan la primera vez (Firefox, Vivaldi, Brave, Opera) y por
   último Chromium, que normalmente no lo trae. Se puede fijar uno en
   Preferencias → Seguridad → «Navegador para el DRM».
4. **Seguridad**: solo direcciones `http(s)`, pasadas como argumentos (nunca por un shell);
   dentro de un AppImage se limpia el entorno para que el otro navegador no cargue las
   bibliotecas de Brújula. En una ventana privada el botón avisa de que la página
   **no será privada** en el otro navegador.
5. `enable-encrypted-media` queda **activado** en las vistas: en una compilación de WebKit
   que tenga EME (Clear Key, o un Widevine que alguien haya conectado) funciona sin más cambios.

Se apaga en Preferencias → Seguridad → «Avisar del contenido con DRM».

## Camino a DRM dentro de Brújula

Ninguna es barata. Lo comprobado hasta ahora:

| Vía | Qué exige | Estado |
| --- | --- | --- |
| **WebKitGTK propio de Nodalix** (`ENABLE_ENCRYPTED_MEDIA` + `ENABLE_THUNDER`, Thunder/OpenCDM y un plugin Widevine) | Compilar WebKit entero (horas, mucha RAM). El plugin de Widevine que usa Thunder en RDK/WPE (`widevine-rdk`) carga la biblioteca de **Widevine CE** (`widevine_ce_cdm_shared`), que Google entrega **con contrato** y con identidad de dispositivo (keybox, aprovisionamiento). No es el `libwidevinecdm.so` de Chrome. ([Documentación de RDK](https://developer.rdkcentral.com/documentation/documentation/rdk_video_documentation/components/widevine-rdk/)) | **Bloqueada sin un acuerdo con Google**: sin esa biblioteca no hay qué cargar, por mucho WebKit propio que se compile. Sin verificar contra Netflix o Disney+ |
| **Motor Chromium (CEF) solo para pestañas con DRM** | Integrar CEF en la app GTK (ventana aparte o incrustada), unos 300 MB más. CEF no trae Widevine: se usa el `libwidevinecdm.so` del Chrome o Firefox del usuario (`--widevine-cdm-path`), como hacen Electron y otros | Técnicamente posible; hay que revisar los términos de Widevine de uso con un CDM copiado |
| **El camino actual** (abrir en otro navegador) | Nada | Funciona hoy |
