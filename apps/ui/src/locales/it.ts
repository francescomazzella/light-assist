import type { Catalog } from "../i18n.js";

/**
 * Italian. Each English text the app says, and how it says it in Italian.
 *
 * The person at the other computer is "il tuo assistente"; the reader is addressed as "tu". System
 * names are the ones Windows and macOS themselves use in Italian. `{names}` stay as they are.
 */
export const it: Catalog = {
  // The console: asking for assistance
  "Request assistance with this computer": "Richiedi assistenza per questo computer",
  "Someone you trust can see your screen from their own browser, with nothing to install.": "Una persona di cui ti fidi può vedere il tuo schermo dal proprio browser, senza installare nulla.",
  "You get an address to tell them.": "Ricevi un indirizzo da comunicarle.",
  "They open it and read you a code, which you type here.": "Lo apre e ti legge un codice, che tu scrivi qui.",
  "They see your screen. You decide whether they may also use the mouse, keyboard or a terminal.": "Vede il tuo schermo. Sei tu a decidere se può usare anche mouse, tastiera o un terminale.",
  Start: "Avvia",
  "1. Ask your assistant to open this in a browser": "1. Chiedi al tuo assistente di aprire questo indirizzo in un browser",
  "Copy address": "Copia indirizzo",
  "Address copied": "Indirizzo copiato",
  "Without the internet, on this network: {address}": "Senza internet, su questa rete: {address}",
  "Light Assist's website cannot be reached from this computer right now, so this address only works on this network. {address} will work again when it can.":
    "In questo momento il sito di Light Assist non è raggiungibile da questo computer, quindi questo indirizzo funziona solo su questa rete. {address} tornerà a funzionare appena possibile.",
  "Or: {address}": "Oppure: {address}",
  "If it does not load: {address}": "Se non si carica: {address}",
  "Over Tailscale: {address}": "Tramite Tailscale: {address}",
  "2. Type the code they see": "2. Scrivi il codice che vede",
  Allow: "Consenti",
  "Nobody has opened the address yet.": "Nessuno ha ancora aperto l'indirizzo.",
  "1 person has the page open and sees a code.": "1 persona ha la pagina aperta e vede un codice.",
  "{count} people have the page open. Each sees a different code.": "{count} persone hanno la pagina aperta. Ognuna vede un codice diverso.",
  "Allowing lets that person see your screen. Anything more is a separate question.": "Consentendo, quella persona potrà vedere il tuo schermo. Tutto il resto ti verrà chiesto a parte.",
  "Closes by itself in {time} if nobody connects.": "Si chiude da sola tra {time} se nessuno si connette.",
  "Stop the request": "Annulla la richiesta",
  "Light Assist is not reachable.": "Light Assist non è raggiungibile.",
  "That code does not match anyone waiting. Ask your assistant to read it again.": "Quel codice non corrisponde a nessuno in attesa. Chiedi al tuo assistente di rileggerlo.",
  "Enter the 6-digit code your assistant sees.": "Inserisci il codice di 6 cifre che vede il tuo assistente.",
  "You are no longer requesting assistance.": "Non stai più richiedendo assistenza.",
  "An assistant is already connected.": "C'è già un assistente connesso.",
  "Light Assist could not open a network port on this computer.": "Light Assist non è riuscito ad aprire una porta di rete su questo computer.",
  "No assistant is connected.": "Nessun assistente è connesso.",
  "Your assistant's page is a different version.": "La pagina del tuo assistente è di una versione diversa.",
  "That device is not trusted.": "Quel dispositivo non è attendibile.",

  // The console: during a session
  "Your assistant is connected": "Il tuo assistente è connesso",
  "A trusted device is connected": "Un dispositivo attendibile è connesso",
  "“{device}” connected by itself, as you allow trusted devices to.": "“{device}” si è connesso da solo, come consenti ai dispositivi attendibili.",
  "From {address}": "Da {address}",
  "From {address}, on a network this computer is on": "Da {address}, su una rete a cui è collegato questo computer",
  "Slow connection: your assistant sees a simpler picture until it improves.": "Connessione lenta: il tuo assistente vede un'immagine semplificata finché non migliora.",
  "Screen and sound": "Schermo e audio",
  "Not yet allowed in System Settings: they see nothing": "Non ancora consentito in Impostazioni di Sistema: non vede nulla",
  "The connection to your assistant was lost. If it does not come back in half a minute, the session ends.":
    "La connessione con il tuo assistente si è interrotta. Se non riprende entro mezzo minuto, la sessione termina.",
  "Connecting the two computers…": "Collegamento dei due computer…",
  "The two computers cannot reach each other. Your assistant has to be on this network, or on your Tailscale network.":
    "I due computer non riescono a raggiungersi. Il tuo assistente deve essere su questa rete, oppure sulla tua rete Tailscale.",
  "Seeing your screen, hearing what this computer plays": "Vede il tuo schermo e sente ciò che riproduce questo computer",
  "Mouse and keyboard": "Mouse e tastiera",
  "Allowed here, but not yet in System Settings": "Consentito qui, ma non ancora in Impostazioni di Sistema",
  "Controlling this computer": "Sta controllando questo computer",
  "Controlling this computer, sharing copied text": "Sta controllando questo computer; il testo copiato è condiviso",
  "Not allowed": "Non consentito",
  "Take control back": "Riprendi il controllo",
  Terminal: "Terminale",
  "Open on this computer": "Aperto su questo computer",
  "Close terminal": "Chiudi terminale",
  "To disconnect at once from anywhere, press {key}.": "Per disconnettere subito, da qualsiasi finestra, premi {key}.",
  Disconnect: "Disconnetti",
  "Let your assistant use the mouse and keyboard?": "Consentire al tuo assistente di usare mouse e tastiera?",
  "They will be able to click and type on this computer.": "Potrà fare clic e scrivere su questo computer.",
  "Text that either of you copies can then be pasted on the other computer.": "Il testo copiato da uno dei due potrà essere incollato sull'altro computer.",
  "You can take control back at any time.": "Puoi riprendere il controllo in qualsiasi momento.",
  "Your Mac also asks for your permission: turn Light Assist on under Accessibility in System Settings, or their mouse and keyboard will do nothing.":
    "Anche il tuo Mac chiede il tuo permesso: attiva Light Assist in Accessibilità, in Impostazioni di Sistema, altrimenti il suo mouse e la sua tastiera non avranno effetto.",
  "Open System Settings": "Apri Impostazioni di Sistema",
  "Don't allow": "Non consentire",
  "Let your assistant open a terminal?": "Consentire al tuo assistente di aprire un terminale?",
  "They will be able to run any command as you, and you will not see what they type. You can close the terminal at any time.":
    "Potrà eseguire qualsiasi comando a tuo nome, e tu non vedrai ciò che scrive. Puoi chiudere il terminale in qualsiasi momento.",
  "Trust this assistant's browser?": "Considerare attendibile il browser di questo assistente?",
  "Done. You can remove it again in the settings.": "Fatto. Puoi rimuoverlo dalle impostazioni.",
  "It will be able to connect and use the mouse and keyboard while you are away, without a code and without asking you, from this network or over Tailscale. Do this only for a device of your own or of someone you trust completely.":
    "Potrà connettersi e usare mouse e tastiera mentre non ci sei, senza codice e senza chiedertelo, da questa rete o tramite Tailscale. Fallo solo per un tuo dispositivo o per quello di una persona di cui ti fidi completamente.",
  "A name for it, such as Anna's laptop": "Un nome, ad esempio Portatile di Anna",
  "A name for this device": "Un nome per questo dispositivo",
  Trust: "Considera attendibile",
  "Let Light Assist show your screen": "Consenti a Light Assist di mostrare il tuo schermo",
  "Your Mac asks for your permission before an app can show your screen to anyone. Turn Light Assist on under Screen & System Audio Recording in System Settings, then quit Light Assist and open it again.":
    "Il tuo Mac chiede il tuo permesso prima che un'app possa mostrare il tuo schermo a qualcuno. Attiva Light Assist in Registrazione schermo e audio di sistema, in Impostazioni di Sistema, poi esci da Light Assist e riaprilo.",
  "Until then your assistant can connect, but sees nothing.": "Fino ad allora il tuo assistente può connettersi, ma non vede nulla.",

  // The console: when nothing is happening
  "Your last session ended by itself": "L'ultima sessione è terminata da sola",
  "The connection between the two computers was lost and did not come back. That happens when either of them changes network or goes to sleep.":
    "La connessione tra i due computer si è interrotta e non è ripresa. Succede quando uno dei due cambia rete o va in stop.",
  "Your assistant could use this computer but had not done anything for a while.": "Il tuo assistente poteva usare questo computer ma non faceva nulla da un po'.",
  "It reached the longest session time you allow.": "Ha raggiunto la durata massima che consenti per una sessione.",
  "You can change both in the settings.": "Puoi modificare entrambe le cose nelle impostazioni.",
  "Trusted devices can connect": "I dispositivi attendibili possono connettersi",
  "Trusted devices cannot connect right now": "In questo momento i dispositivi attendibili non possono connettersi",
  "While Light Assist is running, the device you trust can connect at {address} and use this computer without asking. You can change that in the settings.":
    "Mentre Light Assist è in esecuzione, il dispositivo che consideri attendibile può connettersi a {address} e usare questo computer senza chiedere. Puoi cambiarlo nelle impostazioni.",
  "While Light Assist is running, the {count} devices you trust can connect at {address} and use this computer without asking. You can change that in the settings.":
    "Mentre Light Assist è in esecuzione, i {count} dispositivi che consideri attendibili possono connettersi a {address} e usare questo computer senza chiedere. Puoi cambiarlo nelle impostazioni.",

  // The console's frame
  "Starting…": "Avvio…",
  "Assistant connected": "Assistente connesso",
  "Waiting for your assistant": "In attesa del tuo assistente",
  "Not sharing": "Nessuna condivisione",
  "Stopping makes this computer invisible on the network again.": "Annullando, questo computer torna invisibile sulla rete.",
  "Nothing on this computer is visible on the network until you request assistance.": "Nulla di questo computer è visibile sulla rete finché non richiedi assistenza.",
  Settings: "Impostazioni",
  Minimize: "Riduci a icona",
  Close: "Chiudi",

  // The settings
  "Ending sessions automatically": "Termine automatico delle sessioni",
  "When your assistant does nothing": "Quando il tuo assistente non fa nulla",
  "Applies while they can use the mouse, keyboard or a terminal, so a forgotten session does not stay open.":
    "Vale mentre può usare mouse, tastiera o un terminale, così una sessione dimenticata non resta aperta.",
  Never: "Mai",
  "After {time}": "Dopo {time}",
  "{count} minutes": "{count} minuti",
  "1 hour": "1 ora",
  "{count} hours": "{count} ore",
  "Longest session": "Durata massima di una sessione",
  "Any session ends after this long, whatever is happening in it.": "Ogni sessione termina dopo questo tempo, qualunque cosa stia accadendo.",
  "No limit": "Nessun limite",
  "The settings could not be loaded or saved.": "Non è stato possibile caricare o salvare le impostazioni.",
  "Copy and paste": "Copia e incolla",
  "Share copied text with an assistant who has control": "Condividi il testo copiato con un assistente che ha il controllo",
  "While your assistant can use the mouse and keyboard, text copied on either computer can be pasted on the other. Never before you allow control, and only text.":
    "Mentre il tuo assistente può usare mouse e tastiera, il testo copiato su uno dei due computer può essere incollato sull'altro. Mai prima che tu consenta il controllo, e solo testo.",
  "When you are away": "Quando non ci sei",
  "Let trusted devices connect by themselves": "Consenti ai dispositivi attendibili di connettersi da soli",
  "A device you trust can then see this computer and use its mouse and keyboard without a code and without anyone here to allow it, from this network or a Tailscale network this computer is on. This computer keeps one address, and stays reachable on the network while Light Assist is running and there is a trusted device. It must be switched on, logged in and unlocked.":
    "Un dispositivo che consideri attendibile potrà vedere questo computer e usarne mouse e tastiera senza codice e senza che qualcuno qui lo consenta, da questa rete o da una rete Tailscale a cui è collegato questo computer. Questo computer mantiene un solo indirizzo e resta raggiungibile sulla rete finché Light Assist è in esecuzione e c'è un dispositivo attendibile. Deve essere acceso, con la sessione aperta e sbloccato.",
  "Use it only on a network you run yourself, such as your home's, or over Tailscale: on a shared network, someone else could get in the same way.":
    "Usalo solo su una rete che gestisci tu, come quella di casa, o tramite Tailscale: su una rete condivisa qualcun altro potrebbe entrare allo stesso modo.",
  "Stop the request before changing this.": "Annulla la richiesta prima di modificare questa opzione.",
  "No device is trusted yet. To trust one, have it assist you in the usual way, then choose Trust in this window while it is connected.":
    "Nessun dispositivo è ancora attendibile. Per aggiungerne uno, fatti assistere da quel dispositivo nel modo consueto, poi scegli Considera attendibile in questa finestra mentre è connesso.",
  "Last connected {date}": "Ultima connessione: {date}",
  "Has not connected yet": "Non si è ancora connesso",
  Remove: "Rimuovi",
  "They connect at {address}, in the browser you trusted.": "Si connettono a {address}, dal browser che hai considerato attendibile.",
  "They connect at {hosted}, or on this network at {address}, in the browser you trusted.":
    "Si connettono a {hosted}, oppure su questa rete a {address}, dal browser che hai considerato attendibile.",
  "They connect at {hosted} (which this computer cannot reach right now), or on this network at {address}, in the browser you trusted.":
    "Si connettono a {hosted} (che in questo momento questo computer non riesce a raggiungere), oppure su questa rete a {address}, dal browser che hai considerato attendibile.",
  "Over Tailscale the address is {address}, and a browser has to be trusted there separately: it keeps what makes it trusted for one address.":
    "Tramite Tailscale l'indirizzo è {address}, e lì un browser va considerato attendibile separatamente: ciò che lo rende attendibile vale per un solo indirizzo.",
  "This computer": "Questo computer",
  "Open at login": "Apri al login",
  "Start with Windows": "Avvia con Windows",
  "Waits in the menu bar, ready when you need assistance. Nothing is visible on the network until you request it.":
    "Resta nella barra dei menu, pronto quando ti serve assistenza. Nulla è visibile sulla rete finché non la richiedi.",
  "Waits in the tray, ready when you need assistance. Nothing is visible on the network until you request it.":
    "Resta nell'area di notifica, pronto quando ti serve assistenza. Nulla è visibile sulla rete finché non la richiedi.",
  "In your Applications folder. To remove it, drag it to the Trash.": "È nella cartella Applicazioni. Per rimuoverlo, trascinalo nel Cestino.",
  "To keep Light Assist, drag it to your Applications folder.": "Per conservare Light Assist, trascinalo nella cartella Applicazioni.",
  "Install Light Assist": "Installa Light Assist",
  "Adds it to the Start menu and to Settings → Apps, so you can find it again or remove it. No administrator rights needed.":
    "Lo aggiunge al menu Start e a Impostazioni → App, così puoi ritrovarlo o rimuoverlo. Non servono diritti di amministratore.",
  "Installing…": "Installazione…",
  "Install on this computer": "Installa su questo computer",
  "Stop the request first.": "Annulla prima la richiesta.",
  "Installed from the Microsoft Store, which keeps it up to date.": "Installato dal Microsoft Store, che lo mantiene aggiornato.",
  "Installed. Remove it from Settings → Apps.": "Installato. Per rimuoverlo vai in Impostazioni → App.",
  "Light Assist could not be installed.": "Non è stato possibile installare Light Assist.",
  Language: "Lingua",
  "The same as this computer": "La stessa di questo computer",
  Done: "Fine",
  "About Light Assist": "Informazioni su Light Assist",
  Version: "Versione",
  Updates: "Aggiornamenti",
  "Brought by the Microsoft Store": "Gestiti dal Microsoft Store",
  "Not checked: this copy was built without a website": "Non controllati: questa copia è stata compilata senza un sito",
  "Version {new} is available": "È disponibile la versione {new}",
  "Get it": "Scaricala",
  "Up to date": "Aggiornato",
  "The website could not be reached": "Non è stato possibile raggiungere il sito",
  "Checking…": "Controllo in corso…",
  License: "Licenza",
  "Free software under the GNU AGPL 3.0": "Software libero con licenza GNU AGPL 3.0",
  "Source code": "Codice sorgente",
  "Privacy policy": "Informativa sulla privacy",
  "A new version of Light Assist is available": "È disponibile una nuova versione di Light Assist",
  "Version {new} is out; this is {current}.": "È uscita la versione {new}; questa è la {current}.",

  // The indicator bar
  "Light Assist: drag to move": "Light Assist: trascina per spostare",
  "Trusted device connected": "Dispositivo attendibile connesso",
  "Screen & sound": "Schermo e audio",
  "Mouse & keyboard": "Mouse e tastiera",
  "Close the terminal": "Chiudi il terminale",
  "The session ends by itself if your assistant does not come back": "La sessione termina da sola se il tuo assistente non torna",
  "Connection lost": "Connessione persa",
  "Your assistant sees a simpler picture until the connection improves": "Il tuo assistente vede un'immagine semplificata finché la connessione non migliora",
  "Slow connection": "Connessione lenta",
  "Open Light Assist": "Apri Light Assist",
  Open: "Apri",
  "Disconnect your assistant ({key} does the same from anywhere)": "Disconnetti il tuo assistente ({key} fa lo stesso da qualsiasi finestra)",
  Stop: "Interrompi",

  // The page the assistant opens: before a session
  "Assist someone": "Assisti qualcuno",
  "Waiting to be let in": "In attesa di essere ammesso",
  "Connecting…": "Connessione…",
  "Not connected": "Non connesso",
  "You are assisting the person at this address. They decide what you can see and do.": "Stai assistendo la persona a questo indirizzo. È lei a decidere cosa puoi vedere e fare.",
  "Read this code to the person you are assisting": "Leggi questo codice alla persona che stai assistendo",
  "Pairing code": "Codice di abbinamento",
  "They type it into Light Assist and choose Allow. Keep this page open; the code is only for you and changes every few minutes.":
    "Lo scrive in Light Assist e sceglie Consenti. Tieni aperta questa pagina: il codice è solo per te e cambia ogni pochi minuti.",
  "Who are you assisting?": "Chi stai assistendo?",
  "Light Assist on their computer shows an address that ends in a few words, such as {example}. Type those words here.":
    "Light Assist, sul suo computer, mostra un indirizzo che termina con alcune parole, come {example}. Scrivi qui quelle parole.",
  "Try again": "Riprova",
  "Need assistance yourself?": "Ti serve assistenza?",
  "Install Light Assist on the computer that needs it. Whoever assists you needs only a browser.": "Installa Light Assist sul computer che ne ha bisogno. A chi ti assiste basta un browser.",
  "From the Microsoft Store": "Dal Microsoft Store",
  "Windows 10 and 11, from the latest release": "Windows 10 e 11, dall'ultima versione pubblicata",
  "Built on your Mac: see how": "Da compilare sul tuo Mac: ecco come",
  Privacy: "Privacy",

  // The page the assistant opens: why a session did not start, or ended
  "This computer is not requesting assistance right now.": "In questo momento questo computer non sta richiedendo assistenza.",
  "Someone else is already assisting, or too many people are waiting.": "Qualcun altro sta già assistendo, oppure troppe persone sono in attesa.",
  "That code is not valid.": "Quel codice non è valido.",
  "The other computer did not understand this page. Reload and try again.": "L'altro computer non ha capito questa pagina. Ricaricala e riprova.",
  "The other computer could not be reached.": "Non è stato possibile raggiungere l'altro computer.",
  "The session has not been allowed yet.": "La sessione non è ancora stata consentita.",
  "This page and Light Assist on the other computer are different versions. Reload this page; if that does not help, they need to update Light Assist.":
    "Questa pagina e Light Assist sull'altro computer sono di versioni diverse. Ricarica la pagina; se non basta, Light Assist va aggiornato sull'altro computer.",
  "The other computer no longer lets this browser connect by itself.": "L'altro computer non consente più a questo browser di connettersi da solo.",
  "Light Assist on the other computer is an older version that this page can no longer work with. They need to update it.":
    "Light Assist sull'altro computer è di una versione più vecchia, con cui questa pagina non funziona più. Va aggiornato.",
  "Light Assist on the other computer is newer than this page. Reload the page and try again.": "Light Assist sull'altro computer è più recente di questa pagina. Ricarica la pagina e riprova.",
  "A newer connection attempt replaced this one.": "Un tentativo di connessione più recente ha sostituito questo.",
  "The other computer did not answer in time.": "L'altro computer non ha risposto in tempo.",
  "The other computer could not set up the connection.": "L'altro computer non è riuscito a stabilire la connessione.",
  "The session has ended, because nothing was done with it for a while.": "La sessione è terminata perché è rimasta inattiva per un po'.",
  "The session has ended: it reached the time limit set on the other computer.": "La sessione è terminata: ha raggiunto il limite di tempo impostato sull'altro computer.",
  "The session has ended, because the connection between the two computers was lost.": "La sessione è terminata perché la connessione tra i due computer si è interrotta.",
  "The session has ended.": "La sessione è terminata.",
  "This computer is no longer requesting assistance, or the connection was lost.": "Questo computer non sta più richiedendo assistenza, oppure la connessione si è interrotta.",
  "You disconnected.": "Ti sei disconnesso.",

  // The page the assistant opens: the connection
  "Opening a direct connection…": "Apertura di una connessione diretta…",
  Retry: "Riprova",
  "Their Mac has not allowed Light Assist to show its screen yet.": "Il suo Mac non ha ancora consentito a Light Assist di mostrare lo schermo.",
  "Their screen cannot be shown right now. It may be locked or turned off.": "In questo momento non è possibile mostrare il suo schermo. Potrebbe essere bloccato o spento.",
  "The connection to the other computer was lost. Trying again…": "La connessione con l'altro computer si è interrotta. Nuovo tentativo…",
  "The connection to the other computer was lost.": "La connessione con l'altro computer si è interrotta.",
  "The connection could not be set up.": "Non è stato possibile stabilire la connessione.",
  "The answer did not come from the computer you were paired with, so no connection was made. Reload and try again.":
    "La risposta non è arrivata dal computer a cui eri abbinato, quindi non è stata stabilita alcuna connessione. Ricarica la pagina e riprova.",
  "This computer could not reach theirs. Light Assist connects two computers directly: both have to be on the same network (the same Wi-Fi, for instance) or on the same Tailscale network.":
    "Questo computer non è riuscito a raggiungere il suo. Light Assist collega due computer direttamente: devono essere entrambi sulla stessa rete (lo stesso Wi-Fi, per esempio) o sulla stessa rete Tailscale.",
  "This computer could not reach theirs directly. A firewall on either one may be in the way.":
    "Questo computer non è riuscito a raggiungere direttamente il suo. Potrebbe esserci di mezzo un firewall su uno dei due.",
  "This browser did not say which certificate it uses.": "Questo browser non ha indicato quale certificato usa.",

  // The page the assistant opens: the bar over the screen
  "Viewing their screen": "Stai vedendo il suo schermo",
  "Controlling their computer": "Stai controllando il suo computer",
  "Waiting for them to allow control…": "In attesa che consenta il controllo…",
  "They did not allow control": "Non ha consentito il controllo",
  "Waiting for them to allow the terminal…": "In attesa che consenta il terminale…",
  "They did not allow the terminal": "Non ha consentito il terminale",
  "Request control": "Richiedi il controllo",
  Keys: "Tasti",
  "Press on their computer": "Premi sul suo computer",
  "⌘ acts as Ctrl": "⌘ funziona come Ctrl",
  "Ctrl acts as ⌘": "Ctrl funziona come ⌘",
  "When on, Command with C, V, X, A, Z, Y, S or F acts as Ctrl on their computer. Otherwise Command is their Windows key.":
    "Se attivo, Comando con C, V, X, A, Z, Y, S o F funziona come Ctrl sul suo computer. Altrimenti Comando è il suo tasto Windows.",
  "When on, Ctrl with C, V, X, A, Z, Y, S or F acts as Command on their Mac. Otherwise Ctrl is their Control key.":
    "Se attivo, Ctrl con C, V, X, A, Z, Y, S o F funziona come Comando sul suo Mac. Altrimenti Ctrl è il suo tasto Control.",
  Quality: "Qualità",
  Prefer: "Dai la precedenza a",
  "Sharp text": "Testo nitido",
  "Smooth motion": "Movimento fluido",
  "Fast response": "Risposta rapida",
  Resolution: "Risoluzione",
  "Frame rate": "Fotogrammi al secondo",
  Bitrate: "Bitrate",
  Auto: "Automatico",
  "Their screen's size": "Dimensioni del suo schermo",
  "Now {size} · {fps} FPS · {bitrate}": "Ora {size} · {fps} FPS · {bitrate}",
  More: "Altro",
  "Their screen": "Il suo schermo",
  "Screen {number}": "Schermo {number}",
  main: "principale",
  "Ask for a terminal": "Chiedi un terminale",
  "Show terminal": "Mostra il terminale",
  "Hide terminal": "Nascondi il terminale",
  "Stop being a trusted device": "Smetti di essere un dispositivo attendibile",
  "This browser can connect to their computer while nobody is there. Forgetting it means asking them for a code again.":
    "Questo browser può connettersi al suo computer quando non c'è nessuno. Se vi rinunci, dovrai chiedere di nuovo un codice.",
  "Show stream info": "Mostra i dati della trasmissione",
  "Hear their computer": "Ascolta il suo computer",
  "Click to hear their computer": "Fai clic per ascoltare il suo computer",
  "Mute their computer": "Disattiva l'audio del suo computer",
  "Full screen": "Schermo intero",
  "Exit full screen": "Esci da schermo intero",
  "link {ms} ms": "collegamento {ms} ms",
  "buffered {ms} ms": "buffer {ms} ms",
  "sent {size} at {bitrate}": "inviati {size} a {bitrate}",

  // The page the assistant opens: notices over the screen
  "The connection is slow, so the picture is simpler than usual.": "La connessione è lenta, quindi l'immagine è più semplice del solito.",
  "They now trust this browser: it can connect to this computer by itself, at this same address. Click to dismiss.":
    "Ora considera attendibile questo browser: può connettersi a quel computer da solo, a questo stesso indirizzo. Fai clic per chiudere.",
  "They copied some text. Click here to copy it to this computer.": "Ha copiato del testo. Fai clic qui per copiarlo su questo computer.",
  "Copied from their computer": "Copiato dal suo computer",
  "What was copied on their computer is too much text to bring over.": "Il testo copiato sul suo computer è troppo lungo per essere trasferito.",
  "That is too much text to paste on their computer.": "È troppo testo da incollare sul suo computer.",
  "Their Mac has not been told to let Light Assist show its screen yet. They have to allow it in System Settings, then open Light Assist again.":
    "Il suo Mac non ha ancora il permesso di far mostrare lo schermo a Light Assist. Deve consentirlo in Impostazioni di Sistema e poi riaprire Light Assist.",
  "Their Mac is locked. You can't see or use it until they unlock it themselves.": "Il suo Mac è bloccato. Non puoi vederlo né usarlo finché non lo sblocca di persona.",
  "Windows is showing a permission prompt or the lock screen on their computer. You can't see or use it: they have to answer it themselves.":
    "Sul suo computer Windows sta mostrando una richiesta di autorizzazione o la schermata di blocco. Non puoi vederla né usarla: deve rispondere di persona.",
  "Their Mac has not been told to let Light Assist use its mouse and keyboard yet, so yours do nothing. They can allow it in System Settings.":
    "Il suo Mac non ha ancora il permesso di far usare mouse e tastiera a Light Assist, quindi i tuoi non hanno effetto. Può consentirlo in Impostazioni di Sistema.",
  "The window in front on their computer runs as administrator, so your mouse and keyboard don't work in it.":
    "La finestra in primo piano sul suo computer è eseguita come amministratore, quindi il tuo mouse e la tua tastiera lì non funzionano.",
  "They are using their mouse or keyboard. Yours will work again when they stop.": "Sta usando il suo mouse o la sua tastiera. I tuoi riprenderanno a funzionare quando smette.",

  // The terminal panel
  "Terminal on their computer": "Terminale sul suo computer",
  Hide: "Nascondi",
  "The terminal was closed.": "Il terminale è stato chiuso.",
  "Start a new one": "Avviane uno nuovo",
};
