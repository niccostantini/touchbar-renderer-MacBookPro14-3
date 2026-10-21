# touchbar-renderer

Renderer per la Touch Bar dei MacBook Pro con chip T1 (2016–2017) su Linux,
sopra [t1bridge](https://github.com/standardagents/t1bridge), con slider in
stile macOS e controlli contestuali. Provato su MacBookPro14,3 con
Ubuntu 26.04 e GNOME 50.

Il procedimento completo (firmware T1, t1bridge su Ubuntu, problemi e
soluzioni) è in **[docs/PROCESSO.md](docs/PROCESSO.md)**.

## La barra

![Barra normale](docs/img/1-normale.png)

- **Esc** sempre a sinistra; con **Fn** premuto: Esc, F1–F12.
- **Ritaglio** apre lo strumento screenshot di GNOME (tasto Stamp).
- **Lucchetto** blocca lo schermo; **luna** = Non disturbare di GNOME.
- **Bluetooth** acceso/spento (blu quando è acceso), come l'interruttore di GNOME;
  tenuto premuto mezzo secondo apre l'elenco dei dispositivi.
- **Microfono**: compare solo mentre un'app registra; rosso e barrato se muto.
- **Brano in riproduzione** (MPRIS): tocca per pausa, trascina per spostarti.
- **Sole / tastiera / volume** aprono uno slider a tutta barra: tocca, oppure
  tocca e trascina subito. Chiusura con ✕, Fn o dopo 4 s.
- **Altoparlante**: sceglie l'uscita audio senza aprire le Impostazioni.
- A schermo bloccato o spento la barra è nera e non risponde.

### Tutte le schermate

Musica in riproduzione e microfono in uso:

![Musica e microfono](docs/img/2-musica-e-microfono.png)

Un titolo troppo lungo scorre lentamente (marquee), con una pausa all'inizio:

![Titolo lungo](docs/img/2b-titolo-lungo-che-scorre.png)

Muto, Non disturbare attivo, microfono muto, brano in pausa, tasto premuto:

![Stati attivi](docs/img/3-muto-non-disturbare-pausa.png)

Con Fn premuto:

![Tasti Fn](docs/img/4-tasti-fn.png)

Slider della luminosità dello schermo, della tastiera e del volume:

![Slider luminosità](docs/img/5-slider-luminosita.png)
![Slider tastiera](docs/img/6-slider-tastiera.png)
![Slider volume](docs/img/7-slider-volume.png)

Scelta dell'uscita audio (quella attiva nel gradiente):

![Uscite audio](docs/img/8-uscite-audio.png)

Tenendo premuto il lucchetto per 1,8 secondi la barra si riempie e blocca lo schermo (staccando prima non succede nulla):

![Tieni premuto per bloccare](docs/img/9-tieni-premuto-per-bloccare.png)

Tenendo premuto il tasto Bluetooth si apre l'elenco dei dispositivi: prima
quelli associati (in blu quelli connessi), poi quelli rilevati nelle vicinanze
(più scuri). Un tocco connette, disconnette o associa; mentre il comando è in
corso il dispositivo è blu scuro. Se il Bluetooth è spento viene acceso:

![Dispositivi Bluetooth](docs/img/10-dispositivi-bluetooth.png)

Le immagini si rigenerano con `cargo run --release --bin preview -- docs/img`.

## Installazione

Serve t1bridge installato (vedi `docs/PROCESSO.md`). Il renderer dipende dai
crate di t1bridge tramite percorso, quindi `t1bridge` va clonato **accanto**
a questo repository (`setup/01-build-t1bridge.sh` lo fa da solo):

```
~/t1bridge/            # t1bridge con setup/t1bridge-local.patch applicata
~/touchbar-renderer/   # questo repository
```

```sh
sh setup/03-install-renderer.sh
```

Per tornare al renderer integrato di t1bridge:

```sh
rm ~/.config/t1bridge/renderer && systemctl --user restart t1-touchbar.service
```

## Struttura

| File | Ruolo |
|---|---|
| `src/session.rs` | Connessione al servizio hardware, ciclo principale, invio dei fotogrammi |
| `src/connection.rs` | Protocollo e negoziazione con `t1-touchbar-hw` |
| `src/bar.rs` | Layout, disegno e riconoscimento dei tocchi |
| `src/input.rs` | Tocchi → tap, slider, trascinamento sul brano, Touch ID |
| `src/slider.rs` | Stato degli slider, chiusura automatica, passi di luminosità |
| `src/context.rs` | Thread di contesto: player, volume, microfono, blocco schermo |
| `src/bluetooth.rs` | Dispositivi Bluetooth (BlueZ via `busctl`, comandi con `bluetoothctl`) |
| `src/mpris.rs`, `src/audio.rs`, `src/screen.rs`, `src/system.rs` | Integrazione con GNOME/PipeWire via `busctl`, `wpctl`, `gsettings`, `loginctl` |
| `src/levels.rs` | Livelli di luminosità letti da sysfs |
| `src/text.rs` | Testo con i font di sistema (`fontdue`) |
| `src/art/` | Icone e forme; `stock_*` copiati da t1bridge (MIT) |
| `src/bin/preview.rs` | Disegna tutte le schermate in PNG: `cargo run --release --bin preview -- /tmp/anteprime` |
| `setup/` | Script di compilazione/installazione di t1bridge, patch locale, provider |

## Sviluppo

```sh
cargo test --release
cargo run --release --bin preview -- /tmp/anteprime   # controllo grafico senza hardware
sh setup/03-install-renderer.sh                       # prova sulla barra
journalctl --user -u t1-touchbar -f                   # log
```

Note di progetto:
- La luminosità dello schermo si regola solo a passi (tasti luminosità),
  perché GNOME 50 annulla le scritture dirette: lo slider impara la
  dimensione del passo e si ferma entro mezzo passo dal valore scelto.
- Tutti i comandi esterni girano su thread separati: il ciclo di disegno
  non aspetta mai un sottoprocesso.
- `panic = "abort"`: se il renderer si blocca, t1bridge passa al renderer
  integrato invece di lasciare la barra in uno stato a metà.

## Licenza

MIT. Parti derivate da t1bridge (MIT): vedi `LICENSE-t1bridge`.

## Tema desktop "Leo Camp"

Tema GNOME ispirato allo stile di brave.com/leo: crema e nero, serif da
rivista (Fraunces), card con bordo spesso e ombra netta, gradiente
magenta → arancio, accenti viola; variante scura automatica; dock in basso;
icone WhiteSur rosa (o Papirus con cartelle magenta); sfondi generati.
Icone WhiteSur: `git clone https://github.com/vinceliuice/WhiteSur-icon-theme && WhiteSur-icon-theme/install.sh -t pink`.

```sh
sudo apt install gnome-shell-extensions papirus-icon-theme   # una volta
sh theme/install.sh                                          # poi logout/login
sh ~/.config/leocamp/restore.sh                              # per tornare com'era
```

Mailspring (snap) ha due temi abbinati, chiaro e scuro, in modalità
"Automatico" (segue lo stile di GNOME): `sh theme/mailspring/install.sh`,
poi chiudere e riaprire Mailspring; `--undo` per tornare com'era.
