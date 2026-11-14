# DRM dentro de Brújula con CEF (diseño y plan)

Estado: **diseño, sin código ni prueba**. Daniel eligió esta vía el 2026-11-14 tras ver que
WebKitGTK no puede (ver [drm.md](drm.md)). Lo escrito aquí viene de documentación y de leer el
código de CEF; nada se ha ejecutado, porque la nube donde se escribió no alcanza las descargas de
CEF ni de Chrome.

## Idea

Una **segunda aplicación**, `brujula-drm`, que lleva Chromium (CEF) y se abre solo para las
pestañas que necesitan DRM. Brújula la lanza desde la nota de DRM, que hoy abre otro navegador:
el botón dirá «Abrir en el reproductor de Brújula». Si `brujula-drm` no está instalada, o no hay
Widevine, la nota sigue ofreciendo Chrome o Firefox como ahora.

Se descarta incrustar CEF dentro de la ventana GTK4: en Linux el modo con ventana propia de CEF
se incrusta por X11 y no por Wayland, y el modo fuera de pantalla (OSR) pierde aceleración y
vídeo protegido. Una ventana propia con el *Views framework* de CEF funciona igual en X11 y en
Wayland (Ozone).

## Cómo se carga Widevine

CEF **no trae Widevine**. Una aplicación CEF lo carga desde una carpeta con `manifest.json` y la
biblioteca (`--widevine-cdm-path`), que es lo que hay en las instalaciones de Chrome:

- `/opt/google/chrome/WidevineCdm/` y `~/.config/google-chrome/WidevineCdm/<versión>/`
  (con `_platform_specific/linux_x64/libwidevinecdm.so`);
- las carpetas equivalentes de Chromium, Chrome Beta, Edge, Brave y Vivaldi.

El manifiesto debe traer `os`, `arch`, `version` y las listas `x-cdm-*-versions`; el cargador de
CEF rechaza el CDM si faltan o no coinciden con la versión de CEF. Brújula **no descarga ni
redistribuye** Widevine: usa el que ya tiene el usuario y, si no hay ninguno, lo dice y propone
instalar Chrome. (Firefox guarda el suyo en `~/.mozilla/firefox/gmp-widevinecdm/`, con otro
formato de manifiesto: sin comprobar que sirva.)

## Lo que puede no salir

1. **Códecs**. Las compilaciones públicas de CEF no llevan H.264/AAC (ffmpeg «Chrome branding»).
   Muchos servicios pueden caer a VP9/AV1, pero no está comprobado servicio por servicio. Si hace
   falta, hay que compilar CEF con `proprietary_codecs=true`: un día de máquina y decenas de GB.
2. **Licencia**. Usar un CDM copiado de Chrome en otra aplicación puede no estar permitido por
   los términos de Widevine. Hay que leerlos antes de publicarlo; mientras sea de uso propio y
   sin redistribuir el binario, el riesgo es el de la copia, no el del reparto.
3. **Comprobación del anfitrión**. El CDM comprueba quién lo carga en algunos casos; Electron y
   CEF lo cargan hoy, pero un cambio de Google puede romperlo sin aviso.
4. **Calidad**. Será Widevine **L3**: hasta 720p en la mayoría de plataformas, sin 4K ni HDR.
5. **Tamaño**: unos 300 MB de CEF más la aplicación.

## Fases

0. **Comprobar en un equipo real** (lo que no se pudo desde la nube): con la CEF de ejemplo
   (`cefsimple`) y el CDM de Chrome, abrir una demo de DRM de prueba y luego Netflix y Spotify.
   Anotar qué códecs faltan. Si falla aquí, no se sigue.
1. **Ayudante** `brujula-drm` (crate nueva, fuera de los miembros por defecto del espacio de
   trabajo, para que `cargo build` normal no necesite CEF): ventana CEF, cabecera mínima
   (atrás, adelante, recargar, dirección, pantalla completa), perfil propio en
   `~/.local/share/brujula/drm-profile` (se entra una vez en cada servicio), busca el CDM
   como arriba, solo abre `http(s)`.
2. **Integración**: `browser/drm.rs` prefiere `brujula-drm` si existe y hay CDM; ajuste
   «Navegador para el DRM» con «Reproductor de Brújula».
3. **Empaquetado** en nodalix-os (`packaging/nodalix-brujula-drm`), con el CEF descargado y
   comprobado por hash; dependencia opcional de Brújula.

La fase 0 necesita un equipo con red a `cef-builds.spotifycdn.com` y Chrome instalado.
