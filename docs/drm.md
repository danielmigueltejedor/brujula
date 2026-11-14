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
- En Linux de escritorio Widevine solo llega a **L3** (por software), sea cual sea el navegador;
  la calidad máxima la fija cada servicio (ver «Calidad máxima» más abajo).

Por eso **Brújula no puede prometer DRM dentro de la ventana** con el motor actual. Lo que sí hace:

## Calidad máxima (investigado 2026-11-14)

- **L1 (hardware) no existe en Linux de escritorio**: ningún navegador de Linux lo consigue; exige
  un entorno de ejecución de confianza certificado por Google (Android certificado, Android TV,
  Chromebooks con DRM por hardware). Waydroid solo da L3 porque L1 se certifica por dispositivo
  ([issue de Waydroid](https://github.com/waydroid/waydroid/issues/1408),
  [explicación de los niveles](https://www.forasoft.com/learn/video-streaming/articles-streaming/widevine-l1-l2-l3)).
- **Netflix**: según su [ayuda oficial](https://help.netflix.com/en/node/30081), Chrome, Firefox y
  Edge en **Linux llegan a 1080p** (4K solo en Windows que cumpla requisitos, Safari en Mac y
  ChromeOS compatible). Así que con L3 ya se ve a 1080p, en un navegador de esa lista.
- **Disney+**: los navegadores de ordenador se quedan en 720p
  ([fuente](https://www.starryhope.com/chromebooks/netflix-chromebook-streaming-caps/)); el 4K es
  de las apps. Es política del servicio, no del navegador.
- **Prime Video**: en Linux, definición estándar; el HD se promete en Windows y macOS (hay un
  proyecto que lo consigue ejecutando el Brave de Windows con Wine,
  [prime-wine](https://github.com/NelloKudo/prime-wine); no lo recomendamos: está en el límite de
  las condiciones del servicio).
- Lo que sube de verdad la calidad es **ver en otro dispositivo**: Android TV/Google TV/Chromecast,
  Fire TV, televisores, Chromebook Plus (Netflix 4K), Windows (4K con PlayReady) o macOS (Safari).

Por eso, para Netflix en Linux, **un Chrome o Firefox de verdad ya da el máximo posible**; un motor
Chromium propio no daría más y podría dar menos (el servicio decide por el navegador).

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
