# Licenciar Widevine: qué se sabe y borrador de contacto

Estado: **borrador, no enviado a nadie**. Investigado el 2026-11-14 con documentación pública de
Google; lo que no aparece en ella está marcado como «sin confirmar». Nada de lo de abajo se ha
probado ni preguntado a Google.

## Qué dice Google (fuentes oficiales)

- Hace falta un **contrato**: «A license agreement is required for the use of Widevine products or
  services» ([Widevine, visión general](https://developers.google.com/widevine/drm/overview)).
- Quien lo pide para integrarlo en un dispositivo escribe por la
  [página de contacto](https://developers.google.com/widevine/contact/support). La documentación
  no publica criterios de elegibilidad.
- Pasos para tener acceso a los repositorios del cliente ([acceso](https://developers.google.com/widevine/access)):
  1. Cuenta de Google con **correo de empresa** (no vale gmail ni outlook).
  2. Firmar el **Widevine Master License Agreement** (puede haber una conversación técnica antes).
  3. Google concede acceso al repositorio que corresponda, evaluado por separado para cada uno.
     Para **L1** la biblioteca **OEMCrypto la da el fabricante del chip**, no Google.
- Coste: Google dice que «no cobra ninguna tarifa por el uso de sus productos y servicios»; no
  dice nada de otros costes del contrato, de la certificación ni del hardware.
- Los componentes de aprovisionamiento, *keybox* y OEMCrypto «son específicos de la integración en
  cada dispositivo»: no hay un paquete genérico para un sistema operativo.

## Qué exige L1 (hardware)

Claves, descifrado y vídeo decodificado dentro de un entorno de ejecución de confianza (TEE) con
ruta de vídeo segura hasta la pantalla, arranque verificado y certificación del dispositivo; el
4K suele pedir además HDCP 2.2
([niveles](https://www.forasoft.com/learn/video-streaming/articles-streaming/widevine-l1-l2-l3)).
La certificación es **por dispositivo**: por eso Waydroid, con un árbol de dispositivo genérico,
se queda en L3 ([Waydroid #1408](https://github.com/waydroid/waydroid/issues/1408)).

## Qué quedó sin resolver

- **Plazos, importe del contrato y criterios de aceptación: sin confirmar.** Google no los publica.
- **¿Existe un programa para un navegador o una distribución Linux de escritorio?** Sin
  confirmar. En la documentación pública solo aparecen fabricantes de dispositivos, de chips y
  navegadores grandes. Mozilla, Brave y otros tienen acuerdos, pero sus condiciones no son públicas.
- **Hardware con TEE certificable** (sin confirmar con fuentes): los PC x86 normales no traen un
  OEMCrypto utilizable; los Chromebooks con DRM por hardware lo logran con Intel y Google
  juntos ([Starry Hope](https://www.starryhope.com/chromebooks/netflix-chromebook-streaming-caps/)).
  OP-TEE (el TEE de código abierto de los SoC ARM) existe, pero que sirva para L1 depende de que
  el fabricante del chip aporte OEMCrypto y de que Google lo certifique.
- **¿Hay alguna distribución u OEM Linux de escritorio con L1?** No he encontrado ninguna. Los
  Chromebooks (ChromeOS) y Android/Android TV son los casos conocidos, y los certifica Google.
- **Aunque se consiguiera L1**, cada servicio decide la calidad por su cuenta: Disney+ limita los
  navegadores de ordenador a 720p y Prime Video da definición estándar en Linux
  ([ver docs/drm.md](drm.md#calidad-máxima-investigado-2026-11-14)); sus apps y sus propios
  acuerdos mandan, no el nivel de Widevine.
- Existe un camino intermedio para **L3** en aplicaciones de escritorio: la firma VMP de
  [castLabs](https://castlabs.com/security/widevine-certification/) (su Electron libre ya viene
  aprobado por Google), pero lo que publica es para Windows y macOS. No dice nada de Linux.

## Lo que recomiendo preguntar

Primero lo barato y con más posibilidades, y solo después L1:

1. ¿Qué programa o contrato aplica a una **distribución Linux de código abierto** y a un navegador
   suyo (Brújula, WebKitGTK; posible motor Chromium) para reproducir contenido Widevine L3?
2. ¿Pueden usar el CDM de Chrome una aplicación de tercero en Linux de escritorio, y con qué
   condiciones (verificación del anfitrión, firma)?
3. ¿Es viable L1 en PC x86 de escritorio con Linux? ¿Con qué fabricantes de chip y qué
   certificación?
4. Plazos, requisitos de la empresa y coste del contrato (no el de Widevine, que dicen que es
   gratis: el de integración y pruebas).

## Borrador del mensaje (en inglés, para el formulario de contacto de Widevine)

> **Para completar antes de enviar:** nombre legal de la empresa, dirección de correo
> corporativa (no personal), web y persona de contacto. Hay que enviarlo desde la cuenta de esa
> empresa: Google no admite correos personales para esto.

```
Subject: Widevine for a Linux desktop distribution and its browser (Nodalix / Brújula)

Hello Widevine team,

We are <COMPANY NAME>, the maker of Nodalix, an open-source Linux desktop
distribution based on Arch Linux (https://github.com/getnodalia/nodalix-os), and of
Brújula, its web browser (https://github.com/getnodalia/brujula), written in Rust
on top of WebKitGTK.

Our users cannot play Widevine-protected video in Brújula today: distribution builds
of WebKitGTK ship without Encrypted Media Extensions and without a Widevine CDM.
Right now we only hand those pages to a Widevine-capable third-party browser. We
would like to understand the right, licensed way to support DRM playback ourselves.

Could you tell us:

1. Which program or agreement applies to a Linux desktop distribution and to its
   own browser that wants to play Widevine content (L3), and who qualifies?
2. Whether a third-party application on desktop Linux may load the CDM that ships
   with Chrome, and under what conditions (host verification / signing).
3. Whether L1 is feasible on x86 desktop PCs running Linux, and what it would take
   (chipset vendors, OEMCrypto, device certification).
4. The expected timeline, the requirements for the company, and any costs of the
   integration and certification process (we understand Widevine itself does not
   charge fees).

We are happy to share technical details and the source code of both projects.

Thank you,
<NAME>, <ROLE>
<COMPANY NAME> · <CORPORATE EMAIL> · <WEB>
```

Enlace del formulario: <https://developers.google.com/widevine/contact/support>.
