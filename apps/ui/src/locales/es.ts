import type { Catalog } from "../i18n.js";

/**
 * Spanish, as written in Spain (es-ES). Each English text the app says, and how it says it in Spanish.
 *
 * The person at the other computer is "tu asistente"; the reader is addressed as "tú". System names
 * are the ones Windows and macOS themselves use in Spanish. `{names}` stay as they are.
 */
export const es: Catalog = {
  // The console: asking for assistance
  "Request assistance with this computer": "Solicita asistencia con este ordenador",
  "Someone you trust can see your screen from their own browser, with nothing to install.": "Una persona de confianza puede ver tu pantalla desde su navegador, sin instalar nada.",
  "You get an address to tell them.": "Recibes una dirección para comunicársela.",
  "They open it and read you a code, which you type here.": "La abre y te lee un código, que escribes aquí.",
  "They see your screen. You decide whether they may also use the mouse, keyboard or a terminal.": "Ve tu pantalla. Tú decides si también puede usar el ratón, el teclado o un terminal.",
  Start: "Empezar",
  "1. Ask your assistant to open this in a browser": "1. Pide a tu asistente que abra esta dirección en un navegador",
  "Copy address": "Copiar dirección",
  "Address copied": "Dirección copiada",
  "Without the internet, on this network: {address}": "Sin internet, en esta red: {address}",
  "Light Assist's website cannot be reached from this computer right now, so this address only works on this network. {address} will work again when it can.":
    "Ahora mismo este ordenador no puede acceder al sitio web de Light Assist, así que esta dirección solo funciona en esta red. {address} volverá a funcionar en cuanto sea posible.",
  "Or: {address}": "O bien: {address}",
  "If it does not load: {address}": "Si no carga: {address}",
  "Over Tailscale: {address}": "A través de Tailscale: {address}",
  "2. Type the code they see": "2. Escribe el código que ve",
  Allow: "Permitir",
  "Nobody has opened the address yet.": "Nadie ha abierto la dirección todavía.",
  "1 person has the page open and sees a code.": "1 persona tiene la página abierta y ve un código.",
  "{count} people have the page open. Each sees a different code.": "{count} personas tienen la página abierta. Cada una ve un código distinto.",
  "Allowing lets that person see your screen. Anything more is a separate question.": "Al permitirlo, esa persona podrá ver tu pantalla. Cualquier otra cosa se te preguntará aparte.",
  "Closes by itself in {time} if nobody connects.": "Se cierra sola en {time} si nadie se conecta.",
  "Stop the request": "Cancelar la solicitud",
  "Light Assist is not reachable.": "No se puede acceder a Light Assist.",
  "That code does not match anyone waiting. Ask your assistant to read it again.": "Ese código no corresponde a nadie en espera. Pide a tu asistente que vuelva a leerlo.",
  "Enter the 6-digit code your assistant sees.": "Escribe el código de 6 cifras que ve tu asistente.",
  "You are no longer requesting assistance.": "Ya no estás solicitando asistencia.",
  "An assistant is already connected.": "Ya hay un asistente conectado.",
  "Light Assist could not open a network port on this computer.": "Light Assist no ha podido abrir un puerto de red en este ordenador.",
  "No assistant is connected.": "No hay ningún asistente conectado.",
  "Your assistant's page is a different version.": "La página de tu asistente es de otra versión.",
  "That device is not trusted.": "Ese dispositivo no es de confianza.",

  // The console: during a session
  "Your assistant is connected": "Tu asistente está conectado",
  "A trusted device is connected": "Hay un dispositivo de confianza conectado",
  "“{device}” connected by itself, as you allow trusted devices to.": "“{device}” se ha conectado por sí solo, como permites a los dispositivos de confianza.",
  "From {address}": "Desde {address}",
  "From {address}, on a network this computer is on": "Desde {address}, en una red a la que está conectado este ordenador",
  "Slow connection: your assistant sees a simpler picture until it improves.": "Conexión lenta: tu asistente ve una imagen simplificada hasta que mejore.",
  "Screen and sound": "Pantalla y sonido",
  "Not yet allowed in System Settings: they see nothing": "Aún no permitido en Ajustes del Sistema: no ve nada",
  "The connection to your assistant was lost. If it does not come back in half a minute, the session ends.":
    "Se ha perdido la conexión con tu asistente. Si no vuelve en medio minuto, la sesión termina.",
  "Connecting the two computers…": "Conectando los dos ordenadores…",
  "The two computers cannot reach each other. Your assistant has to be on this network, or on your Tailscale network.":
    "Los dos ordenadores no consiguen comunicarse. Tu asistente tiene que estar en esta red o en tu red de Tailscale.",
  "Seeing your screen, hearing what this computer plays": "Ve tu pantalla y oye lo que reproduce este ordenador",
  "Mouse and keyboard": "Ratón y teclado",
  "Allowed here, but not yet in System Settings": "Permitido aquí, pero aún no en Ajustes del Sistema",
  "Controlling this computer": "Está controlando este ordenador",
  "Controlling this computer, sharing copied text": "Está controlando este ordenador; el texto copiado se comparte",
  "Not allowed": "No permitido",
  "Take control back": "Recuperar el control",
  Terminal: "Terminal",
  "Open on this computer": "Abierto en este ordenador",
  "Close terminal": "Cerrar terminal",
  "To disconnect at once from anywhere, press {key}.": "Para desconectar al instante desde cualquier ventana, pulsa {key}.",
  Disconnect: "Desconectar",
  "Let your assistant use the mouse and keyboard?": "¿Permitir que tu asistente use el ratón y el teclado?",
  "They will be able to click and type on this computer.": "Podrá hacer clic y escribir en este ordenador.",
  "Text that either of you copies can then be pasted on the other computer.": "El texto que copie cualquiera de los dos se podrá pegar en el otro ordenador.",
  "You can take control back at any time.": "Puedes recuperar el control en cualquier momento.",
  "Your Mac also asks for your permission: turn Light Assist on under Accessibility in System Settings, or their mouse and keyboard will do nothing.":
    "Tu Mac también te pide permiso: activa Light Assist en Accesibilidad, en Ajustes del Sistema, o su ratón y su teclado no harán nada.",
  "Open System Settings": "Abrir Ajustes del Sistema",
  "Don't allow": "No permitir",
  "Let your assistant open a terminal?": "¿Permitir que tu asistente abra un terminal?",
  "They will be able to run any command as you, and you will not see what they type. You can close the terminal at any time.":
    "Podrá ejecutar cualquier orden en tu nombre, y no verás lo que escribe. Puedes cerrar el terminal en cualquier momento.",
  "Trust this assistant's browser?": "¿Marcar como de confianza el navegador de este asistente?",
  "Done. You can remove it again in the settings.": "Hecho. Puedes quitarlo en los ajustes.",
  "It will be able to connect and use the mouse and keyboard while you are away, without a code and without asking you, from this network or over Tailscale. Do this only for a device of your own or of someone you trust completely.":
    "Podrá conectarse y usar el ratón y el teclado cuando no estés, sin código y sin preguntarte, desde esta red o a través de Tailscale. Hazlo solo con un dispositivo tuyo o de alguien de total confianza.",
  "A name for it, such as Anna's laptop": "Un nombre, por ejemplo Portátil de Ana",
  "A name for this device": "Un nombre para este dispositivo",
  Trust: "Marcar como de confianza",
  "Let Light Assist show your screen": "Permite que Light Assist muestre tu pantalla",
  "Your Mac asks for your permission before an app can show your screen to anyone. Turn Light Assist on under Screen & System Audio Recording in System Settings, then quit Light Assist and open it again.":
    "Tu Mac te pide permiso antes de que una app pueda mostrar tu pantalla a alguien. Activa Light Assist en Grabación de pantalla y audio del sistema, en Ajustes del Sistema, y luego sal de Light Assist y vuelve a abrirlo.",
  "Until then your assistant can connect, but sees nothing.": "Hasta entonces tu asistente puede conectarse, pero no ve nada.",

  // The console: when nothing is happening
  "Your last session ended by itself": "Tu última sesión terminó sola",
  "The connection between the two computers was lost and did not come back. That happens when either of them changes network or goes to sleep.":
    "Se perdió la conexión entre los dos ordenadores y no volvió. Ocurre cuando uno de ellos cambia de red o entra en reposo.",
  "Your assistant could use this computer but had not done anything for a while.": "Tu asistente podía usar este ordenador, pero llevaba un rato sin hacer nada.",
  "It reached the longest session time you allow.": "Alcanzó la duración máxima de sesión que permites.",
  "You can change both in the settings.": "Puedes cambiar ambas cosas en los ajustes.",
  "Trusted devices can connect": "Los dispositivos de confianza pueden conectarse",
  "Trusted devices cannot connect right now": "Ahora mismo los dispositivos de confianza no pueden conectarse",
  "While Light Assist is running, the device you trust can connect at {address} and use this computer without asking. You can change that in the settings.":
    "Mientras Light Assist esté abierto, el dispositivo de confianza puede conectarse en {address} y usar este ordenador sin preguntar. Puedes cambiarlo en los ajustes.",
  "While Light Assist is running, the {count} devices you trust can connect at {address} and use this computer without asking. You can change that in the settings.":
    "Mientras Light Assist esté abierto, los {count} dispositivos de confianza pueden conectarse en {address} y usar este ordenador sin preguntar. Puedes cambiarlo en los ajustes.",

  // The console's frame
  "Starting…": "Iniciando…",
  "Assistant connected": "Asistente conectado",
  "Waiting for your assistant": "Esperando a tu asistente",
  "Not sharing": "Sin compartir",
  "Stopping makes this computer invisible on the network again.": "Al cancelar, este ordenador vuelve a ser invisible en la red.",
  "Nothing on this computer is visible on the network until you request assistance.": "Nada de este ordenador es visible en la red hasta que solicitas asistencia.",
  Settings: "Ajustes",
  Minimize: "Minimizar",
  Close: "Cerrar",

  // The settings
  "Ending sessions automatically": "Fin automático de las sesiones",
  "When your assistant does nothing": "Cuando tu asistente no hace nada",
  "Applies while they can use the mouse, keyboard or a terminal, so a forgotten session does not stay open.":
    "Se aplica mientras puede usar el ratón, el teclado o un terminal, para que una sesión olvidada no quede abierta.",
  Never: "Nunca",
  "After {time}": "Tras {time}",
  "{count} minutes": "{count} minutos",
  "1 hour": "1 hora",
  "{count} hours": "{count} horas",
  "Longest session": "Duración máxima de una sesión",
  "Any session ends after this long, whatever is happening in it.": "Toda sesión termina tras este tiempo, pase lo que pase en ella.",
  "No limit": "Sin límite",
  "The settings could not be loaded or saved.": "No se han podido cargar o guardar los ajustes.",
  "Copy and paste": "Copiar y pegar",
  "Share copied text with an assistant who has control": "Compartir el texto copiado con un asistente que tiene el control",
  "While your assistant can use the mouse and keyboard, text copied on either computer can be pasted on the other. Never before you allow control, and only text.":
    "Mientras tu asistente pueda usar el ratón y el teclado, el texto copiado en un ordenador se podrá pegar en el otro. Nunca antes de que permitas el control, y solo texto.",
  "When you are away": "Cuando no estás",
  "Let trusted devices connect by themselves": "Permitir que los dispositivos de confianza se conecten por sí solos",
  "A device you trust can then see this computer and use its mouse and keyboard without a code and without anyone here to allow it, from this network or a Tailscale network this computer is on. This computer keeps one address, and stays reachable on the network while Light Assist is running and there is a trusted device. It must be switched on, logged in and unlocked.":
    "Un dispositivo de confianza podrá ver este ordenador y usar su ratón y su teclado sin código y sin que nadie aquí lo permita, desde esta red o desde una red de Tailscale a la que esté conectado este ordenador. Este ordenador mantiene una sola dirección y sigue accesible en la red mientras Light Assist esté abierto y haya un dispositivo de confianza. Debe estar encendido, con la sesión iniciada y desbloqueado.",
  "Use it only on a network you run yourself, such as your home's, or over Tailscale: on a shared network, someone else could get in the same way.":
    "Úsalo solo en una red que gestiones tú, como la de casa, o a través de Tailscale: en una red compartida, otra persona podría entrar del mismo modo.",
  "Stop the request before changing this.": "Cancela la solicitud antes de cambiar esto.",
  "No device is trusted yet. To trust one, have it assist you in the usual way, then choose Trust in this window while it is connected.":
    "Aún no hay ningún dispositivo de confianza. Para añadir uno, deja que te asista de la forma habitual y luego elige Marcar como de confianza en esta ventana mientras está conectado.",
  "Last connected {date}": "Última conexión: {date}",
  "Has not connected yet": "Aún no se ha conectado",
  Remove: "Quitar",
  "They connect at {address}, in the browser you trusted.": "Se conectan en {address}, desde el navegador que marcaste como de confianza.",
  "They connect at {hosted}, or on this network at {address}, in the browser you trusted.":
    "Se conectan en {hosted}, o en esta red en {address}, desde el navegador que marcaste como de confianza.",
  "They connect at {hosted} (which this computer cannot reach right now), or on this network at {address}, in the browser you trusted.":
    "Se conectan en {hosted} (al que este ordenador no puede acceder ahora mismo), o en esta red en {address}, desde el navegador que marcaste como de confianza.",
  "Over Tailscale the address is {address}, and a browser has to be trusted there separately: it keeps what makes it trusted for one address.":
    "A través de Tailscale la dirección es {address}, y allí el navegador debe marcarse como de confianza por separado: lo que lo hace de confianza vale para una sola dirección.",
  "This computer": "Este ordenador",
  "Open at login": "Abrir al iniciar sesión",
  "Start with Windows": "Iniciar con Windows",
  "Waits in the menu bar, ready when you need assistance. Nothing is visible on the network until you request it.":
    "Espera en la barra de menús, listo para cuando necesites asistencia. Nada es visible en la red hasta que la solicites.",
  "Waits in the tray, ready when you need assistance. Nothing is visible on the network until you request it.":
    "Espera en el área de notificación, listo para cuando necesites asistencia. Nada es visible en la red hasta que la solicites.",
  "In your Applications folder. To remove it, drag it to the Trash.": "Está en la carpeta Aplicaciones. Para eliminarlo, arrástralo a la Papelera.",
  "To keep Light Assist, drag it to your Applications folder.": "Para conservar Light Assist, arrástralo a la carpeta Aplicaciones.",
  "Install Light Assist": "Instalar Light Assist",
  "Adds it to the Start menu and to Settings → Apps, so you can find it again or remove it. No administrator rights needed.":
    "Lo añade al menú Inicio y a Configuración → Aplicaciones, para que puedas volver a encontrarlo o desinstalarlo. No se necesitan permisos de administrador.",
  "Installing…": "Instalando…",
  "Install on this computer": "Instalar en este ordenador",
  "Stop the request first.": "Cancela primero la solicitud.",
  "Installed from the Microsoft Store, which keeps it up to date.": "Instalado desde Microsoft Store, que lo mantiene actualizado.",
  "Installed. Remove it from Settings → Apps.": "Instalado. Puedes desinstalarlo en Configuración → Aplicaciones.",
  "Light Assist could not be installed.": "No se ha podido instalar Light Assist.",
  Language: "Idioma",
  "The same as this computer": "El mismo que este ordenador",
  Done: "Listo",
  "About Light Assist": "Acerca de Light Assist",
  Version: "Versión",
  Updates: "Actualizaciones",
  "Brought by the Microsoft Store": "Las gestiona Microsoft Store",
  "Not checked: this copy was built without a website": "No se comprueban: esta copia se compiló sin sitio web",
  "Version {new} is available": "La versión {new} está disponible",
  "Get it": "Descargar",
  "Up to date": "Actualizado",
  "The website could not be reached": "No se ha podido acceder al sitio web",
  "Checking…": "Comprobando…",
  License: "Licencia",
  "Free software under the GNU AGPL 3.0": "Software libre con licencia GNU AGPL 3.0",
  "Source code": "Código fuente",
  "Privacy policy": "Política de privacidad",
  "A new version of Light Assist is available": "Hay una nueva versión de Light Assist disponible",
  "Version {new} is out; this is {current}.": "Ya ha salido la versión {new}; esta es la {current}.",

  // The indicator bar
  "Light Assist: drag to move": "Light Assist: arrastra para mover",
  "Trusted device connected": "Dispositivo de confianza conectado",
  "Screen & sound": "Pantalla y sonido",
  "Mouse & keyboard": "Ratón y teclado",
  "Close the terminal": "Cerrar el terminal",
  "The session ends by itself if your assistant does not come back": "La sesión termina sola si tu asistente no vuelve",
  "Connection lost": "Conexión perdida",
  "Your assistant sees a simpler picture until the connection improves": "Tu asistente ve una imagen simplificada hasta que mejore la conexión",
  "Slow connection": "Conexión lenta",
  "Open Light Assist": "Abrir Light Assist",
  Open: "Abrir",
  "Disconnect your assistant ({key} does the same from anywhere)": "Desconectar a tu asistente ({key} hace lo mismo desde cualquier ventana)",
  Stop: "Detener",

  // The page the assistant opens: before a session
  "Assist someone": "Asistir a alguien",
  "Waiting to be let in": "Esperando a que te dejen entrar",
  "Connecting…": "Conectando…",
  "Not connected": "Sin conexión",
  "You are assisting the person at this address. They decide what you can see and do.": "Estás asistiendo a la persona de esta dirección. Ella decide qué puedes ver y hacer.",
  "Read this code to the person you are assisting": "Lee este código a la persona a la que asistes",
  "Pairing code": "Código de emparejamiento",
  "They type it into Light Assist and choose Allow. Keep this page open; the code is only for you and changes every few minutes.":
    "Lo escribe en Light Assist y elige Permitir. Mantén esta página abierta: el código es solo para ti y cambia cada pocos minutos.",
  "Who are you assisting?": "¿A quién estás asistiendo?",
  "Light Assist on their computer shows an address that ends in a few words, such as {example}. Type those words here.":
    "Light Assist, en su ordenador, muestra una dirección que termina en unas palabras, como {example}. Escríbelas aquí.",
  "Try again": "Volver a intentarlo",
  "Need assistance yourself?": "¿Necesitas asistencia?",
  "Install Light Assist on the computer that needs it. Whoever assists you needs only a browser.": "Instala Light Assist en el ordenador que la necesita. A quien te asiste le basta un navegador.",
  "From the Microsoft Store": "Desde Microsoft Store",
  "Windows 10 and 11, from the latest release": "Windows 10 y 11, de la última versión publicada",
  "Built on your Mac: see how": "Se compila en tu Mac: así se hace",
  Privacy: "Privacidad",
  "Terms of use": "Condiciones de uso",

  // The page the assistant opens: why a session did not start, or ended
  "This computer is not requesting assistance right now.": "Ahora mismo este ordenador no está solicitando asistencia.",
  "Someone else is already assisting, or too many people are waiting.": "Otra persona ya está asistiendo, o hay demasiadas personas esperando.",
  "That code is not valid.": "Ese código no es válido.",
  "The other computer did not understand this page. Reload and try again.": "El otro ordenador no ha entendido esta página. Recárgala y vuelve a intentarlo.",
  "The other computer could not be reached.": "No se ha podido acceder al otro ordenador.",
  "The session has not been allowed yet.": "La sesión aún no se ha permitido.",
  "This page and Light Assist on the other computer are different versions. Reload this page; if that does not help, they need to update Light Assist.":
    "Esta página y Light Assist en el otro ordenador son de versiones distintas. Recarga la página; si no basta, hay que actualizar Light Assist en el otro ordenador.",
  "The other computer no longer lets this browser connect by itself.": "El otro ordenador ya no permite que este navegador se conecte por sí solo.",
  "Light Assist on the other computer is an older version that this page can no longer work with. They need to update it.":
    "Light Assist en el otro ordenador es una versión antigua con la que esta página ya no funciona. Hay que actualizarlo.",
  "Light Assist on the other computer is newer than this page. Reload the page and try again.": "Light Assist en el otro ordenador es más reciente que esta página. Recárgala y vuelve a intentarlo.",
  "A newer connection attempt replaced this one.": "Un intento de conexión más reciente ha sustituido a este.",
  "The other computer did not answer in time.": "El otro ordenador no ha respondido a tiempo.",
  "The other computer could not set up the connection.": "El otro ordenador no ha podido establecer la conexión.",
  "The session has ended, because nothing was done with it for a while.": "La sesión ha terminado porque llevaba un rato sin actividad.",
  "The session has ended: it reached the time limit set on the other computer.": "La sesión ha terminado: alcanzó el límite de tiempo fijado en el otro ordenador.",
  "The session has ended, because the connection between the two computers was lost.": "La sesión ha terminado porque se perdió la conexión entre los dos ordenadores.",
  "The session has ended.": "La sesión ha terminado.",
  "This computer is no longer requesting assistance, or the connection was lost.": "Este ordenador ya no está solicitando asistencia, o se ha perdido la conexión.",
  "You disconnected.": "Te has desconectado.",

  // The page the assistant opens: the connection
  "Opening a direct connection…": "Abriendo una conexión directa…",
  Retry: "Reintentar",
  "Their Mac has not allowed Light Assist to show its screen yet.": "Su Mac aún no ha permitido que Light Assist muestre la pantalla.",
  "Their screen cannot be shown right now. It may be locked or turned off.": "Ahora mismo no se puede mostrar su pantalla. Puede estar bloqueada o apagada.",
  "The connection to the other computer was lost. Trying again…": "Se ha perdido la conexión con el otro ordenador. Volviendo a intentarlo…",
  "The connection to the other computer was lost.": "Se ha perdido la conexión con el otro ordenador.",
  "The connection could not be set up.": "No se ha podido establecer la conexión.",
  "The answer did not come from the computer you were paired with, so no connection was made. Reload and try again.":
    "La respuesta no vino del ordenador con el que te emparejaste, así que no se ha establecido la conexión. Recarga la página y vuelve a intentarlo.",
  "This computer could not reach theirs. Light Assist connects two computers directly: both have to be on the same network (the same Wi-Fi, for instance) or on the same Tailscale network.":
    "Este ordenador no ha podido comunicarse con el suyo. Light Assist conecta dos ordenadores directamente: ambos tienen que estar en la misma red (la misma Wi-Fi, por ejemplo) o en la misma red de Tailscale.",
  "This computer could not reach theirs directly. A firewall on either one may be in the way.":
    "Este ordenador no ha podido comunicarse directamente con el suyo. Puede que un cortafuegos en alguno de los dos lo impida.",
  "This browser did not say which certificate it uses.": "Este navegador no ha indicado qué certificado usa.",

  // The page the assistant opens: the bar over the screen
  "Viewing their screen": "Viendo su pantalla",
  "Controlling their computer": "Controlando su ordenador",
  "Waiting for them to allow control…": "Esperando a que permita el control…",
  "They did not allow control": "No ha permitido el control",
  "Waiting for them to allow the terminal…": "Esperando a que permita el terminal…",
  "They did not allow the terminal": "No ha permitido el terminal",
  "Request control": "Solicitar el control",
  Keys: "Teclas",
  "Press on their computer": "Pulsar en su ordenador",
  "⌘ acts as Ctrl": "⌘ actúa como Ctrl",
  "Ctrl acts as ⌘": "Ctrl actúa como ⌘",
  "When on, Command with C, V, X, A, Z, Y, S or F acts as Ctrl on their computer. Otherwise Command is their Windows key.":
    "Si está activado, Comando con C, V, X, A, Z, Y, S o F actúa como Ctrl en su ordenador. Si no, Comando es su tecla de Windows.",
  "When on, Ctrl with C, V, X, A, Z, Y, S or F acts as Command on their Mac. Otherwise Ctrl is their Control key.":
    "Si está activado, Ctrl con C, V, X, A, Z, Y, S o F actúa como Comando en su Mac. Si no, Ctrl es su tecla Control.",
  Quality: "Calidad",
  Prefer: "Dar prioridad a",
  "Sharp text": "Texto nítido",
  "Smooth motion": "Movimiento fluido",
  "Fast response": "Respuesta rápida",
  Resolution: "Resolución",
  "Frame rate": "Fotogramas por segundo",
  Bitrate: "Tasa de bits",
  Auto: "Automático",
  "Their screen's size": "Tamaño de su pantalla",
  "Now {size} · {fps} FPS · {bitrate}": "Ahora {size} · {fps} FPS · {bitrate}",
  More: "Más",
  "Their screen": "Su pantalla",
  "Screen {number}": "Pantalla {number}",
  main: "principal",
  "Ask for a terminal": "Pedir un terminal",
  "Show terminal": "Mostrar el terminal",
  "Hide terminal": "Ocultar el terminal",
  "Stop being a trusted device": "Dejar de ser un dispositivo de confianza",
  "This browser can connect to their computer while nobody is there. Forgetting it means asking them for a code again.":
    "Este navegador puede conectarse a su ordenador cuando no hay nadie. Si renuncias, tendrás que volver a pedir un código.",
  "Show stream info": "Mostrar datos de la transmisión",
  "Hear their computer": "Oír su ordenador",
  "Click to hear their computer": "Haz clic para oír su ordenador",
  "Mute their computer": "Silenciar su ordenador",
  "Full screen": "Pantalla completa",
  "Exit full screen": "Salir de pantalla completa",
  "link {ms} ms": "enlace {ms} ms",
  "buffered {ms} ms": "búfer {ms} ms",
  "sent {size} at {bitrate}": "enviado {size} a {bitrate}",

  // The page the assistant opens: notices over the screen
  "The connection is slow, so the picture is simpler than usual.": "La conexión es lenta, así que la imagen es más sencilla de lo habitual.",
  "They now trust this browser: it can connect to this computer by itself, at this same address. Click to dismiss.":
    "Ahora este navegador es de confianza: puede conectarse a ese ordenador por sí solo, en esta misma dirección. Haz clic para cerrar.",
  "They copied some text. Click here to copy it to this computer.": "Ha copiado un texto. Haz clic aquí para copiarlo en este ordenador.",
  "Copied from their computer": "Copiado de su ordenador",
  "What was copied on their computer is too much text to bring over.": "El texto copiado en su ordenador es demasiado largo para traerlo.",
  "That is too much text to paste on their computer.": "Es demasiado texto para pegarlo en su ordenador.",
  "Their Mac has not been told to let Light Assist show its screen yet. They have to allow it in System Settings, then open Light Assist again.":
    "Su Mac aún no permite que Light Assist muestre la pantalla. Tiene que permitirlo en Ajustes del Sistema y volver a abrir Light Assist.",
  "Their Mac is locked. You can't see or use it until they unlock it themselves.": "Su Mac está bloqueado. No puedes verlo ni usarlo hasta que lo desbloquee en persona.",
  "Windows is showing a permission prompt or the lock screen on their computer. You can't see or use it: they have to answer it themselves.":
    "Windows está mostrando una solicitud de permiso o la pantalla de bloqueo en su ordenador. No puedes verla ni usarla: tiene que responder en persona.",
  "Their Mac has not been told to let Light Assist use its mouse and keyboard yet, so yours do nothing. They can allow it in System Settings.":
    "Su Mac aún no permite que Light Assist use su ratón y su teclado, así que los tuyos no hacen nada. Puede permitirlo en Ajustes del Sistema.",
  "The window in front on their computer runs as administrator, so your mouse and keyboard don't work in it.":
    "La ventana en primer plano de su ordenador se ejecuta como administrador, así que tu ratón y tu teclado no funcionan en ella.",
  "They are using their mouse or keyboard. Yours will work again when they stop.": "Está usando su ratón o su teclado. Los tuyos volverán a funcionar cuando pare.",

  // The terminal panel
  "Terminal on their computer": "Terminal en su ordenador",
  Hide: "Ocultar",
  "The terminal was closed.": "Se ha cerrado el terminal.",
  "Start a new one": "Abrir uno nuevo",
};
