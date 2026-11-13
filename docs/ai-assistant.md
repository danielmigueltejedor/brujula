# Asistente de IA (opcional)

Brújula puede hablar con la IA que elijas: ChatGPT (OpenAI), Claude, Gemini, un modelo local con Ollama o cualquier servidor compatible con la API de OpenAI. **Viene apagado**: mientras no lo actives en Preferencias → IA no hay botón, ni panel, ni se envía nada a nadie.

## Qué hace

- **Panel del asistente** (botón ✨ de la barra, menú → «Asistente de IA» o `Ctrl+Mayús+A`). Conversa, y si se lo permites puede ver tus pestañas, leer páginas, buscar en la web y abrir pestañas.
- **Buscador gestionado por la IA** (opcional, otro interruptor): en el Spotlight aparece primero «Preguntar a {proveedor}: …». Una pregunta («cómo se llama el rey de España») la responde la IA sin abrir nada; una tarea («búscame sitios baratos para viajar») le deja usar la web y abrir pestañas, con tus permisos. La búsqueda normal sigue ahí como segunda fila. El formulario de la página nueva no pasa por la IA.

## Qué ve la IA

Solo lo que le das y lo que el panel muestra en la línea «Puede ver…»:

| Herramienta | Qué envía | Permiso |
|---|---|---|
| Listar pestañas | título y dirección de las pestañas | siempre (si el asistente está activo) |
| Leer una página | el texto visible (hasta un límite) y los enlaces/botones | pregunta una vez por sitio y conversación; las pestañas que abrió la propia IA no preguntan |
| Buscar en la web | la consulta | abre una pestaña con tu buscador |
| Abrir / cambiar / cerrar pestaña | una dirección | pregunta, salvo «Permitirlo en toda la conversación»; solo cierra las pestañas que abrió ella |
| Pulsar / escribir en una página | el elemento y el texto | desactivado por defecto; si lo activas, **siempre** pregunta |

Nunca se envían: contraseñas, campos de pago, campos ocultos, el valor de ningún campo de formulario, ventanas privadas, páginas `brujula://` ni `file:`, ni los sitios que excluyas. Las direcciones privadas (localhost, red local) no se pueden abrir desde la IA.

## Seguridad

- **Claves** en el llavero del sistema (`secret-tool`, paquete `libsecret`). Nunca en un archivo ni en la configuración. Solo viajan en una cabecera HTTP al proveedor. Sin llavero, no se puede guardar una clave (Ollama no la necesita).
- **Destino fijo**: ChatGPT, Claude y Gemini usan su dirección oficial. Un servidor propio debe ser `https`, o `http` solo en este equipo; sin usuario ni parámetros; no se siguen redirecciones.
- **El texto de una página es un dato, no una orden**. Va marcado como contenido no fiable. Si la IA ya leyó una página y luego pide abrir una dirección que no estaba en lo leído, se pregunta aunque hayas dado permiso general (defensa contra inyección de instrucciones).
- Límites por conversación: 14 pasos y 8 pestañas abiertas. El botón de parar cancela al momento.
- La lectura y la interacción se pueden apagar por separado; abrir pestañas pregunta siempre salvo que elijas dejarlo hacer sin preguntar.
- Las conversaciones solo viven en memoria; al cerrar el panel o empezar otra se olvidan.

## Ajustes

`ai.json` en la carpeta del perfil (sin la clave). Preferencias → IA: proveedor, modelo, dirección (solo «Otro servidor»), clave (Poner/Cambiar/Quitar), «Probar conexión», permisos, buscador y sitios excluidos.

## Qué no está verificado

Se ha probado con un servidor de pruebas compatible con OpenAI bajo un X virtual (flujos, permisos, secretos excluidos, llavero con `secret-tool`). Falta probarlo con ChatGPT, Claude, Gemini y Ollama reales, y con un llavero de escritorio real.
