# Extensión de prueba

`test-extension/` es una extensión MV3 mínima que ejercita lo que Brújula implementa de la API de
Chromium (script y CSS de contenido, mensajes, puertos, storage, tabs, scripting, fetch con permisos
de host, DNR, popup, opciones). `test-extension-page.html` es la página que usa, junto con un
`blocked.js` que la regla DNR bloquea y un servidor en `127.0.0.1:8765` que registra las llamadas
`/log?clave=valor` que hace la extensión. Se instala desde Preferencias → Extensiones → Elegir carpeta.
