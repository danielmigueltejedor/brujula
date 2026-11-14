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
| Leer una página | el texto visible (hasta un límite) y los enlaces/botones | la página que estás mirando y las pestañas que abrió la propia IA no preguntan; otro sitio, una vez por sitio y conversación |
| Buscar en la web | la consulta | nunca pregunta: abre una pestaña con tu buscador |
| Abrir / cambiar / cerrar pestaña | una dirección | por defecto sin preguntar (se puede cambiar a «Preguntar siempre»); una dirección que no salía en lo que leyó se pregunta siempre; solo cierra las pestañas que abrió ella |
| Organizar pestañas (fijar, mover, carpetas, espacios, cerrar) | los números de las pestañas y los nombres que ponga | sin preguntar, salvo que el plan cierre pestañas: entonces enseña la lista y pregunta (Ctrl+Mayús+T recupera las cerradas); se apaga en los ajustes |
| Proponer una tarea programada | nombre, qué hacer, sitios y horario | **siempre** pregunta, enseñando todo; se apaga en los ajustes. Ver «Tareas programadas» |
| Pulsar / escribir en una página | el elemento y el texto | desactivado por defecto; si lo activas, **siempre** pregunta |

Nunca se envían: contraseñas, campos de pago, campos ocultos, el valor de ningún campo de formulario, ventanas privadas, páginas `brujula://` ni `file:`, ni los sitios que excluyas. Las direcciones privadas (localhost, red local) no se pueden abrir desde la IA.

## Tareas programadas

Le puedes pedir «cada mañana mira Ágora y dime si hay novedades». La IA propone la tarea (nombre, qué hacer, los sitios que puede abrir y cuándo: cada N horas, cada día o ciertos días de la semana) y **no se guarda hasta que dices que sí**, viéndola entera. Se guardan en `ai-tasks.json` (en la carpeta del perfil), hasta 12 por perfil.

Una tarea se ejecuta sola, solo con Brújula abierto, cada vez que llega su hora y la IA no está ocupada ni hay una conversación abierta (nunca te interrumpe ni la toma). Si estaba cerrado a su hora, se hace al abrir. Mientras corre:

- **Solo abre y lee los sitios que nombra** (y sus subdominios). Una página que acaba en otro sitio (una redirección) no se lee.
- No ve tus pestañas, no busca, no pulsa, no escribe, no inicia sesión, no organiza y no crea otras tareas: lo demás se rechaza sin preguntar, porque nadie está mirando.
- Cierra las pestañas que abrió y guarda lo que encontró con la tarea; si el panel está abierto lo ves ahí, y si no, sale un aviso del escritorio.
- Lo que lee se envía al proveedor que elegiste, igual que en una conversación. En Ajustes del asistente ves cada tarea con su último resultado, y puedes activarla o apagarla, ejecutarla ya o borrarla.

## Seguridad

- **Claves** en el llavero del sistema (`secret-tool`, paquete `libsecret`). Nunca en un archivo ni en la configuración. Solo viajan en una cabecera HTTP al proveedor. Sin llavero, no se puede guardar una clave (Ollama no la necesita).
- **Destino fijo**: ChatGPT, Claude y Gemini usan su dirección oficial. Un servidor propio debe ser `https`, o `http` solo en este equipo; sin usuario ni parámetros; no se siguen redirecciones.
- **El texto de una página es un dato, no una orden**. Va marcado como contenido no fiable. Si la IA ya leyó una página y luego pide abrir una dirección que no estaba en lo leído, se pregunta aunque hayas dado permiso general (defensa contra inyección de instrucciones).
- Límites por conversación: 14 pasos y 8 pestañas abiertas. El botón de parar cancela al momento.
- La lectura y la interacción se pueden apagar por separado; si prefieres que cada pestaña que abra la IA se pregunte, se cambia en los ajustes.
- Las conversaciones solo viven en memoria; al cerrar el panel o empezar otra se olvidan.

## Ajustes

`ai.json` en la carpeta del perfil (sin la clave). Los mismos ajustes están dentro del panel (botón de los controles de la cabecera, que desliza los ajustes sobre la conversación) y en Preferencias → IA: proveedor, modelo, dirección (solo «Otro servidor»), clave (Poner/Cambiar/Quitar), «Probar conexión», permisos, tareas programadas, buscador y sitios excluidos.

## Qué no está verificado

Se ha probado con un servidor de pruebas compatible con OpenAI bajo un X virtual (flujos, permisos, secretos excluidos, llavero con `secret-tool`). Falta probarlo con ChatGPT, Claude, Gemini y Ollama reales, y con un llavero de escritorio real.
